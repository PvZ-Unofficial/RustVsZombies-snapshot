use std::path::PathBuf;

use clap::Parser;
use rsvz_witness::{WitnessDiffResult, diff_witness_paths};

#[derive(Parser)]
#[command(name = "witness-diff")]
#[command(about = "Compare two RustVsZombies Witness artifacts")]
struct Args {
    left: PathBuf,
    right: PathBuf,
    #[arg(long, requires = "reference_pe")]
    reference_1051: Option<PathBuf>,
    #[arg(long, requires = "reference_1051")]
    reference_pe: Option<PathBuf>,
}

fn main() -> std::process::ExitCode {
    let args = Args::parse();
    let result = diff_witness_paths(
        &args.left,
        &args.right,
        args.reference_1051.as_deref(),
        args.reference_pe.as_deref(),
    );
    match &result {
        WitnessDiffResult::Equal { frames } => println!("witnesses match across {frames} frames"),
        WitnessDiffResult::Different {
            first_frame,
            last_contiguous_frame,
            reconverged_at,
            field,
        } => {
            println!(
                "first difference at frame {first_frame}; contiguous through {last_contiguous_frame}; reconverged at {}; field: {}",
                reconverged_at.map_or_else(|| "not observed".to_owned(), |frame| frame.to_string()),
                field.as_deref().unwrap_or("Full state unavailable"),
            );
            println!(
                "suggested ReplayWindow {{ start: {}, end: {} }}",
                first_frame.saturating_sub(20),
                first_frame.saturating_add(50),
            );
        }
        WitnessDiffResult::Invalid(error) => eprintln!("invalid witness comparison: {error}"),
    }
    std::process::ExitCode::from(result.exit_code())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn replay_references_are_an_optional_pair() {
        assert!(Args::try_parse_from(["witness-diff", "a.json", "b.json"]).is_ok());
        assert!(Args::try_parse_from(["witness-diff", "a.json", "b.json", "--reference-1051", "old-a.json",]).is_err());
    }
}
