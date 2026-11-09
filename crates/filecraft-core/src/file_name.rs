use std::path::{Path, PathBuf};

use chrono::NaiveDateTime;

/// Extension of every exported text file.
pub const OUTPUT_EXTENSION: &str = "txt";

/// Builds the output file name without extension, optionally appending a `_yyyy_MM_dd_HH_mm_ss` timestamp.
pub fn final_file_name(
    output_file_name: &str,
    append_timestamp: bool,
    now: NaiveDateTime,
) -> String {
    if append_timestamp {
        format!("{output_file_name}_{}", now.format("%Y_%m_%d_%H_%M_%S"))
    } else {
        output_file_name.to_owned()
    }
}

/// The path of an export file named `file_name` (without extension) in `destination`.
pub fn output_path(destination: &Path, file_name: &str) -> PathBuf {
    destination.join(format!("{file_name}.{OUTPUT_EXTENSION}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;

    fn sample_time() -> NaiveDateTime {
        NaiveDate::from_ymd_opt(2026, 3, 7)
            .unwrap()
            .and_hms_opt(9, 5, 2)
            .unwrap()
    }

    #[test]
    fn keeps_name_without_timestamp() {
        assert_eq!(final_file_name("tree", false, sample_time()), "tree");
    }

    #[test]
    fn appends_zero_padded_timestamp() {
        assert_eq!(
            final_file_name("tree", true, sample_time()),
            "tree_2026_03_07_09_05_02"
        );
    }

    #[test]
    fn output_path_adds_the_extension() {
        assert_eq!(
            output_path(Path::new("out"), "tree"),
            Path::new("out").join("tree.txt")
        );
    }
}
