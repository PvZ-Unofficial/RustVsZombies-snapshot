use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::fs;
use std::sync::{Mutex, OnceLock};

use iced_x86::{
    Decoder, DecoderOptions, FlowControl, Instruction, InstructionInfoFactory, Mnemonic, OpAccess, OpKind,
    Register as IcedReg,
};
use proc_macro::{Diagnostic, Level};
use proc_macro2::Span;
use sha2::{Digest, Sha256};

use crate::ir::{AbiSpec, CallSpec, InputBinding, WrapperListSpec};
use crate::register::{ByteReg, GpReg, ReturnReg, X87Reg};

const IMAGE_BASE: u32 = 0x0040_0000;
const TEXT_VA: u32 = 0x0040_1000;
const TEXT_SIZE: usize = 0x0025_008c;
const TEXT_SHA256: [u8; 32] = [
    0xa5, 0x3b, 0x1b, 0x79, 0xf1, 0xe7, 0x9f, 0x97, 0x43, 0x97, 0x64, 0x0c, 0x7e, 0x02, 0x6a, 0xf9, 0x13, 0x14, 0x17,
    0xbf, 0xdd, 0x2c, 0x0b, 0x8b, 0x31, 0x36, 0x61, 0x5c, 0xa4, 0x40, 0xfd, 0x7e,
];
const MAX_INSTRUCTIONS: usize = 4096;
const MAX_EDGES: usize = 8192;
const MAX_PROPAGATIONS: usize = 32768;
const MAX_CALL_DEPTH: usize = 64;
const FULL: u32 = u32::MAX;

static IMAGE: OnceLock<Result<TextImage, String>> = OnceLock::new();
static ANALYSIS_CACHE: OnceLock<Mutex<AnalysisCache>> = OnceLock::new();

pub(crate) fn verify_call(spec: &CallSpec) -> syn::Result<()> {
    let image = image(spec.span)?;
    let mut cache = analysis_cache(spec.span)?;
    let report = analyze(image, &spec.abi, spec.ret.as_ref().map(|ret| ret.reg), &mut cache);
    drop(cache);
    finish_report(spec.span, format!("call at {:#010x}", spec.abi.addr), report)
}

pub(crate) fn verify_wrappers(spec: &WrapperListSpec) -> syn::Result<()> {
    let span = spec.items[0].name.span();
    let image = image(span)?;
    let mut cache = analysis_cache(span)?;
    let mut error = None;
    let mut unknown = Vec::new();

    for item in &spec.items {
        let report = analyze(
            image,
            &item.abi,
            item.output.as_ref().map(|output| output.reg),
            &mut cache,
        );
        for message in report.contradictions {
            combine_error(
                &mut error,
                syn::Error::new(
                    item.name.span(),
                    format!("ABI contradiction for `{}`: {message}", item.name),
                ),
            );
        }
        if !report.unknown.is_empty() {
            unknown.push(format!("{}: {}", item.name, report.unknown.join(", ")));
        }
    }
    drop(cache);

    if !unknown.is_empty() {
        Diagnostic::spanned(
            span.unwrap(),
            Level::Warning,
            format!("ABI verification left unknown properties:\n  {}", unknown.join("\n  ")),
        )
        .emit();
    }

    error.map_or(Ok(()), Err)
}

fn analysis_cache(span: Span) -> syn::Result<std::sync::MutexGuard<'static, AnalysisCache>> {
    ANALYSIS_CACHE
        .get_or_init(|| Mutex::new(AnalysisCache::default()))
        .lock()
        .map_err(|_| syn::Error::new(span, "ABI analysis cache lock was poisoned"))
}

fn finish_report(span: Span, subject: String, report: Report) -> syn::Result<()> {
    let mut error = None;
    for message in report.contradictions {
        combine_error(
            &mut error,
            syn::Error::new(span, format!("ABI contradiction for {subject}: {message}")),
        );
    }
    if !report.unknown.is_empty() {
        Diagnostic::spanned(
            span.unwrap(),
            Level::Warning,
            format!(
                "ABI verification for {subject} left unknown properties: {}",
                report.unknown.join(", ")
            ),
        )
        .emit();
    }
    error.map_or(Ok(()), Err)
}

fn combine_error(target: &mut Option<syn::Error>, next: syn::Error) {
    if let Some(error) = target {
        error.combine(next);
    } else {
        *target = Some(next);
    }
}

fn image(span: Span) -> syn::Result<&'static TextImage> {
    IMAGE
        .get_or_init(|| {
            let path = proc_macro::tracked::env_var("RSVZ_PVZ1051_EXE")
                .map_err(|_| "RSVZ_PVZ1051_EXE must point to the original PlantsVsZombies.exe".to_owned())?;
            proc_macro::tracked::path(&path);
            let bytes = fs::read(&path).map_err(|error| format!("failed to read `{path}`: {error}"))?;
            TextImage::from_pe(&bytes)
                .map_err(|error| format!("`{path}` is not the supported PvZ 1.0.0.1051 EXE: {error}"))
        })
        .as_ref()
        .map_err(|message| syn::Error::new(span, message))
}

#[derive(Debug)]
struct TextImage {
    va: u32,
    bytes: Vec<u8>,
}

impl TextImage {
    fn from_pe(bytes: &[u8]) -> Result<Self, String> {
        if bytes.get(..2) != Some(b"MZ") {
            return Err("missing MZ signature".to_owned());
        }
        let pe = read_u32(bytes, 0x3c)? as usize;
        if bytes.get(pe..pe + 4) != Some(b"PE\0\0") {
            return Err("missing PE signature".to_owned());
        }

        let coff = pe + 4;
        if read_u16(bytes, coff)? != 0x014c {
            return Err("image is not i386".to_owned());
        }
        let section_count = read_u16(bytes, coff + 2)? as usize;
        let optional_size = read_u16(bytes, coff + 16)? as usize;
        let optional = coff + 20;
        if read_u16(bytes, optional)? != 0x010b {
            return Err("image is not PE32".to_owned());
        }
        if read_u32(bytes, optional + 28)? != IMAGE_BASE {
            return Err(format!("unexpected image base; expected {IMAGE_BASE:#010x}"));
        }

        let sections = optional + optional_size;
        for index in 0..section_count {
            let section = sections + index * 40;
            let name = bytes
                .get(section..section + 8)
                .ok_or_else(|| "truncated section table".to_owned())?;
            if &name[..5] != b".text" || name[5..].iter().any(|byte| *byte != 0) {
                continue;
            }

            let virtual_size = read_u32(bytes, section + 8)? as usize;
            let rva = read_u32(bytes, section + 12)?;
            let raw_size = read_u32(bytes, section + 16)? as usize;
            let raw_offset = read_u32(bytes, section + 20)? as usize;
            if IMAGE_BASE.checked_add(rva) != Some(TEXT_VA) {
                return Err(format!("unexpected .text address; expected {TEXT_VA:#010x}"));
            }
            if virtual_size != TEXT_SIZE {
                return Err(format!("unexpected .text virtual size; expected {TEXT_SIZE:#x}"));
            }
            if raw_size < virtual_size {
                return Err(".text raw data is shorter than its virtual size".to_owned());
            }
            let text = bytes
                .get(raw_offset..raw_offset + virtual_size)
                .ok_or_else(|| "truncated .text data".to_owned())?;
            if Sha256::digest(text).as_slice() != TEXT_SHA256 {
                return Err("unexpected .text SHA-256".to_owned());
            }
            return Ok(Self {
                va: TEXT_VA,
                bytes: text.to_vec(),
            });
        }

        Err("missing .text section".to_owned())
    }

    #[cfg(test)]
    fn synthetic(va: u32, bytes: &[u8]) -> Self {
        Self {
            va,
            bytes: bytes.to_vec(),
        }
    }

    fn decode(&self, address: u32) -> Option<Instruction> {
        let offset = address.checked_sub(self.va)? as usize;
        let bytes = self.bytes.get(offset..)?;
        let mut decoder = Decoder::with_ip(32, bytes, u64::from(address), DecoderOptions::NONE);
        let instruction = decoder.decode();
        (!instruction.is_invalid() && instruction.len() != 0).then_some(instruction)
    }

    fn read_u8(&self, address: u32) -> Option<u8> {
        let offset = address.checked_sub(self.va)? as usize;
        self.bytes.get(offset).copied()
    }

    fn read_u32(&self, address: u32) -> Option<u32> {
        let offset = address.checked_sub(self.va)? as usize;
        let bytes = self.bytes.get(offset..offset + 4)?;
        Some(u32::from_le_bytes(bytes.try_into().ok()?))
    }

    fn contains(&self, address: u32) -> bool {
        address
            .checked_sub(self.va)
            .is_some_and(|offset| (offset as usize) < self.bytes.len())
    }
}

fn read_u16(bytes: &[u8], offset: usize) -> Result<u16, String> {
    let value = bytes
        .get(offset..offset + 2)
        .ok_or_else(|| format!("truncated u16 at {offset:#x}"))?;
    Ok(u16::from_le_bytes([value[0], value[1]]))
}

fn read_u32(bytes: &[u8], offset: usize) -> Result<u32, String> {
    let value = bytes
        .get(offset..offset + 4)
        .ok_or_else(|| format!("truncated u32 at {offset:#x}"))?;
    Ok(u32::from_le_bytes([value[0], value[1], value[2], value[3]]))
}

mod analysis;

use analysis::{AnalysisCache, Report, analyze};
