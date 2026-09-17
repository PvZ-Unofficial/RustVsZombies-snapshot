use std::fs;
use std::path::{Path, PathBuf};

use proc_macro2::{Delimiter, TokenStream, TokenTree};
use syn::visit::{self, Visit};

fn test_only(meta: &syn::Meta) -> bool {
    match meta {
        syn::Meta::Path(path) => path.is_ident("test"),
        syn::Meta::List(list) => {
            let Ok(conditions) =
                list.parse_args_with(syn::punctuated::Punctuated::<syn::Meta, syn::Token![,]>::parse_terminated)
            else {
                return false;
            };
            if list.path.is_ident("all") {
                conditions.iter().any(test_only)
            } else {
                list.path.is_ident("any") && !conditions.is_empty() && conditions.iter().all(test_only)
            }
        }
        syn::Meta::NameValue(_) => false,
    }
}

fn test_attribute(attribute: &syn::Attribute) -> bool {
    attribute.path().is_ident("cfg") && attribute.parse_args::<syn::Meta>().is_ok_and(|meta| test_only(&meta))
}

#[derive(Default)]
struct ProductionCfg {
    violations: Vec<&'static str>,
}

impl ProductionCfg {
    // syn deliberately leaves macro templates and arguments as tokens.
    fn macro_tokens(&mut self, tokens: TokenStream) {
        if let Ok(items) = syn::parse2::<syn::File>(tokens.clone()) {
            self.visit_file(&items);
            return;
        }
        let tokens = tokens.into_iter().collect::<Vec<_>>();
        for (index, token) in tokens.iter().enumerate() {
            if let TokenTree::Ident(name) = token {
                if name == "cfg"
                    && matches!(tokens.get(index + 1), Some(TokenTree::Punct(punctuation)) if punctuation.as_char() == '!')
                {
                    self.violations.push("cfg!");
                }
            }
            if let TokenTree::Group(group) = token {
                let attribute = group.delimiter() == Delimiter::Bracket
                    && (matches!(tokens.get(index.wrapping_sub(1)), Some(TokenTree::Punct(punctuation)) if punctuation.as_char() == '#')
                        || (matches!(tokens.get(index.wrapping_sub(1)), Some(TokenTree::Punct(punctuation)) if punctuation.as_char() == '!')
                            && matches!(tokens.get(index.wrapping_sub(2)), Some(TokenTree::Punct(punctuation)) if punctuation.as_char() == '#')));
                if attribute {
                    if let Some(TokenTree::Ident(name)) = group.stream().into_iter().next() {
                        if name == "cfg_attr" {
                            self.violations.push("cfg_attr in macro");
                        } else if name == "cfg" {
                            let is_test = match syn::parse2::<syn::Meta>(group.stream()) {
                                Ok(syn::Meta::List(list)) => {
                                    list.parse_args::<syn::Meta>().is_ok_and(|meta| test_only(&meta))
                                }
                                _ => false,
                            };
                            if !is_test {
                                self.violations.push("cfg in macro");
                            }
                        }
                    }
                }
                self.macro_tokens(group.stream());
            }
        }
    }
}

impl<'ast> Visit<'ast> for ProductionCfg {
    fn visit_attribute(&mut self, attribute: &'ast syn::Attribute) {
        if attribute.path().is_ident("cfg_attr") {
            self.violations.push("cfg_attr");
        } else if attribute.path().is_ident("cfg") && !test_attribute(attribute) {
            self.violations.push("cfg");
        }
        visit::visit_attribute(self, attribute);
    }

    fn visit_item(&mut self, item: &'ast syn::Item) {
        let attributes = match item {
            syn::Item::Const(item) => &item.attrs,
            syn::Item::Enum(item) => &item.attrs,
            syn::Item::ExternCrate(item) => &item.attrs,
            syn::Item::Fn(item) => &item.attrs,
            syn::Item::ForeignMod(item) => &item.attrs,
            syn::Item::Impl(item) => &item.attrs,
            syn::Item::Macro(item) => &item.attrs,
            syn::Item::Mod(item) => &item.attrs,
            syn::Item::Static(item) => &item.attrs,
            syn::Item::Struct(item) => &item.attrs,
            syn::Item::Trait(item) => &item.attrs,
            syn::Item::TraitAlias(item) => &item.attrs,
            syn::Item::Type(item) => &item.attrs,
            syn::Item::Union(item) => &item.attrs,
            syn::Item::Use(item) => &item.attrs,
            syn::Item::Verbatim(tokens) => {
                self.macro_tokens(tokens.clone());
                return;
            }
            _ => {
                visit::visit_item(self, item);
                return;
            }
        };
        if !attributes.iter().any(test_attribute) {
            visit::visit_item(self, item);
        }
    }

    fn visit_impl_item_fn(&mut self, item: &'ast syn::ImplItemFn) {
        if !item.attrs.iter().any(test_attribute) {
            visit::visit_impl_item_fn(self, item);
        }
    }

    fn visit_macro(&mut self, item: &'ast syn::Macro) {
        if item.path.segments.last().is_some_and(|segment| segment.ident == "cfg") {
            self.violations.push("cfg!");
        }
        self.macro_tokens(item.tokens.clone());
    }
}

fn collect_sources(directory: &Path, files: &mut Vec<PathBuf>) {
    for entry in fs::read_dir(directory).expect("source directory") {
        let path = entry.expect("source entry").path();
        if path.is_dir() {
            collect_sources(&path, files);
        } else if path.extension().is_some_and(|extension| extension == "rs") {
            files.push(path);
        }
    }
}

#[test]
fn core_and_api_production_cfg_is_confined_to_current_selector() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..");
    let mut files = Vec::new();
    for entry in fs::read_dir(root.join("crates/core")).expect("core crates") {
        let entry = entry.expect("core crate entry");
        if entry.file_name() != "current" && entry.path().join("src").is_dir() {
            collect_sources(&entry.path().join("src"), &mut files);
        }
    }
    collect_sources(&root.join("crates/scripting/api/src"), &mut files);
    for path in files {
        let source = fs::read_to_string(&path).expect("Rust source");
        let syntax = syn::parse_file(&source).unwrap_or_else(|error| panic!("{}: {error}", path.display()));
        let mut check = ProductionCfg::default();
        check.visit_file(&syntax);
        assert!(
            check.violations.is_empty(),
            "production conditional compilation in {}: {:?}",
            path.display(),
            check.violations
        );
    }
}

#[test]
fn production_cfg_gate_checks_syntax_and_macro_templates() {
    for source in [
        "#[cfg(feature = \"runtime\")] fn operation() {}",
        "#[cfg_attr(feature = \"runtime\", inline)] fn operation() {}",
        "fn operation() { let _ = cfg!(feature = \"runtime\"); }",
        "fn operation() { let _ = std::cfg!(feature = \"runtime\"); }",
        "#[cfg(any(test, feature = \"runtime\"))] fn operation() {}",
        "macro_rules! operation { () => { #[cfg(feature = \"runtime\")] fn run() {} } }",
        "macro_rules! operation { ($condition:meta) => { #[cfg($condition)] fn run() {} } }",
        "macro_rules! operation { () => { #[cfg_attr(test, inline)] fn run() {} } }",
        "macro_rules! operation { () => { cfg!(feature = \"runtime\") } }",
    ] {
        let mut check = ProductionCfg::default();
        check.visit_file(&syn::parse_file(source).expect("fixture syntax"));
        assert!(!check.violations.is_empty(), "gate accepted {source}");
    }
    for source in [
        "#[cfg(test)] mod tests { #[cfg(feature = \"fixture\")] fn case() {} }",
        "#[cfg(all(test, feature = \"fixture\"))] mod tests {}",
        "macro_rules! tests { () => { #[cfg(test)] mod tests { #[cfg(feature = \"fixture\")] fn case() {} } } }",
        "fn operation() { let _ = \"#[cfg(feature = runtime)]\"; }",
        "// #[cfg(feature = runtime)]\nfn operation() {}",
    ] {
        let mut check = ProductionCfg::default();
        check.visit_file(&syn::parse_file(source).expect("fixture syntax"));
        assert!(check.violations.is_empty(), "gate rejected {source}");
    }
}
