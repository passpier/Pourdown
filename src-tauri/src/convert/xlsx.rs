use calamine::{open_workbook_auto, Data, Reader};
use chrono::{Days, NaiveDate};
use pulldown_cmark::{Event, Options, Parser, Tag, TagEnd};
use rust_xlsxwriter::Workbook;

use super::ConversionError;

const MAX_ROWS_PER_SHEET: usize = 500;

/// Convert an Excel serial date number to an ISO date string (YYYY-MM-DD).
/// Excel's epoch is 1899-12-30 (accounting for the Lotus 1-2-3 leap-year bug).
fn excel_serial_to_date(serial: i64) -> String {
    NaiveDate::from_ymd_opt(1899, 12, 30)
        .and_then(|epoch| epoch.checked_add_days(Days::new(serial as u64)))
        .map(|d| d.format("%Y-%m-%d").to_string())
        .unwrap_or_else(|| serial.to_string())
}

/// Returns true if the column header text suggests the column holds dates.
fn is_date_header(header: &str) -> bool {
    let lower = header.to_lowercase();
    lower.contains("date") || lower.contains("日期")
}

/// Returns true if `f` looks like a plausible Excel date serial (1970–2099).
fn looks_like_excel_date(f: f64) -> bool {
    f.fract() == 0.0 && f >= 25_569.0 && f <= 73_050.0
}


/// Convert a cell value to a Markdown-safe string.
/// - Newlines are collapsed to a space (GFM tables must be single-line).
/// - Pipe characters are escaped to avoid breaking table structure.
pub fn cell_to_string(cell: &Data) -> String {
    let s = match cell {
        Data::Empty => String::new(),
        Data::String(s) => s.clone(),
        Data::Float(f) => {
            if f.fract() == 0.0 && f.abs() < 1e15 {
                format!("{}", *f as i64)
            } else {
                format!("{}", f)
            }
        }
        Data::Int(i) => format!("{}", i),
        Data::Bool(b) => {
            if *b {
                "TRUE".to_string()
            } else {
                "FALSE".to_string()
            }
        }
        Data::Error(e) => format!("{:?}", e),
        Data::DateTime(dt) => excel_serial_to_date(dt.as_f64() as i64),
        Data::DateTimeIso(s) => s.clone(),
        Data::DurationIso(s) => s.clone(),
    };
    // Collapse cell-internal newlines; escape pipes so table structure is intact.
    s.replace("\r\n", " ")
        .replace('\r', " ")
        .replace('\n', " ")
        .replace('|', "\\|")
}

/// Like `cell_to_string` but also interprets numeric values as ISO dates when
/// the column header indicates a date column.
fn cell_to_string_ctx(cell: &Data, date_col: bool) -> String {
    if date_col {
        match cell {
            Data::Float(f) if looks_like_excel_date(*f) => {
                return excel_serial_to_date(*f as i64);
            }
            Data::Int(i) if looks_like_excel_date(*i as f64) => {
                return excel_serial_to_date(*i);
            }
            Data::DateTime(dt) => {
                return excel_serial_to_date(dt.as_f64() as i64);
            }
            _ => {}
        }
    }
    cell_to_string(cell)
}

/// Convert an Excel file (xlsx/xls/ods/csv) to Markdown.
/// Each sheet becomes a ## heading followed by a GFM table.
/// Rows are capped at MAX_ROWS_PER_SHEET with an inline note if truncated.
pub fn xlsx_to_markdown(path: &str) -> Result<String, ConversionError> {
    let mut workbook = open_workbook_auto(path)
        .map_err(|e| ConversionError(format!("Failed to open spreadsheet: {}", e)))?;

    let sheet_names: Vec<String> = workbook.sheet_names().to_vec();
    let mut output = String::new();

    for sheet_name in sheet_names {
        let range = workbook
            .worksheet_range(&sheet_name)
            .map_err(|e| ConversionError(format!("Failed to read sheet '{}': {}", sheet_name, e)))?;

        let (total_rows, col_count) = range.get_size();

        if col_count == 0 {
            continue;
        }

        if !output.is_empty() {
            output.push('\n');
        }
        output.push_str(&format!("## {}\n\n", sheet_name));

        if total_rows == 0 {
            output.push_str("*(empty sheet)*\n");
            continue;
        }

        let rows_to_read = total_rows.min(MAX_ROWS_PER_SHEET + 1); // +1 for header
        let mut rows_iter = range.rows().take(rows_to_read);

        // --- Header row ---
        let header_raw = match rows_iter.next() {
            Some(row) => row,
            None => continue,
        };

        // Detect which columns are date columns based on header text.
        let date_cols: Vec<bool> = header_raw
            .iter()
            .map(|c| is_date_header(&cell_to_string(c)))
            .collect();

        let header_strs: Vec<String> = header_raw
            .iter()
            .map(|c| cell_to_string(c))
            .collect();

        output.push('|');
        for h in &header_strs {
            output.push_str(&format!(" {} |", h));
        }
        output.push('\n');

        // Separator
        output.push('|');
        for _ in 0..col_count {
            output.push_str(" --- |");
        }
        output.push('\n');

        // --- Data rows: collect first, then merge continuation rows ---
        let mut all_rows: Vec<Vec<String>> = Vec::new();
        let mut data_row_count = 0usize;

        for row in rows_iter {
            if data_row_count >= MAX_ROWS_PER_SHEET {
                break;
            }
            let cells: Vec<String> = row
                .iter()
                .enumerate()
                .map(|(ci, c)| cell_to_string_ctx(c, *date_cols.get(ci).unwrap_or(&false)))
                .collect();
            all_rows.push(cells);
            data_row_count += 1;
        }

        // Merge continuation rows (Option A heuristic):
        // A continuation row is one where:
        //   - its first cell is non-empty (carries a note or partial data)
        //   - all cells past the "split point" are empty in both this row and the prior row
        //     OR: this row's early cells are empty and its later cells have values that fill
        //         gaps left empty in the prior row.
        //
        // Concretely: if this row's cells slot exactly into the empty cells of the prior row
        // (no column has a value in both rows), merge them.
        let merged_rows = merge_continuation_rows(all_rows);

        for row in &merged_rows {
            output.push('|');
            for cell in row {
                output.push_str(&format!(" {} |", cell));
            }
            output.push('\n');
        }

        // Truncation notice
        if total_rows > MAX_ROWS_PER_SHEET + 1 {
            let omitted = total_rows - MAX_ROWS_PER_SHEET - 1;
            output.push_str(&format!(
                "\n> **Note**: {} rows were omitted (showing first {} data rows).\n",
                omitted, MAX_ROWS_PER_SHEET
            ));
        }
    }

    Ok(output)
}

/// Merge "continuation rows" into their preceding row where possible.
///
/// A row is merged upward when every non-empty cell in the current row corresponds
/// to an empty cell in the previous row (i.e. the two rows together fill non-overlapping
/// columns). This handles xlsx patterns where a long description pushes status/type columns
/// onto the next physical row.
fn merge_continuation_rows(rows: Vec<Vec<String>>) -> Vec<Vec<String>> {
    let mut result: Vec<Vec<String>> = Vec::with_capacity(rows.len());

    for row in rows {
        let can_merge = if let Some(prev) = result.last() {
            // Rows must have the same column count to attempt a merge.
            prev.len() == row.len()
                // Every non-empty cell in `row` must land in a slot that is empty in `prev`.
                && row.iter().zip(prev.iter()).all(|(cur, prv)| cur.is_empty() || prv.is_empty())
                // At least one cell in `row` must be non-empty (skip purely blank rows).
                && row.iter().any(|c| !c.is_empty())
                // Require that `row` actually adds something to the missing tail of `prev`.
                // Guard: if `prev` is already completely filled there is nothing to merge.
                && prev.iter().any(|c| c.is_empty())
        } else {
            false
        };

        if can_merge {
            let prev = result.last_mut().unwrap();
            for (p, c) in prev.iter_mut().zip(row.iter()) {
                if p.is_empty() && !c.is_empty() {
                    *p = c.clone();
                }
            }
        } else {
            result.push(row);
        }
    }

    result
}

/// Extract GFM pipe tables from Markdown text.
/// Returns a list of (header_row, data_rows) where each row is Vec<String>.
pub fn extract_tables_from_markdown(markdown: &str) -> Vec<(Vec<String>, Vec<Vec<String>>)> {
    let mut tables = Vec::new();
    let options = Options::ENABLE_TABLES;
    let parser = Parser::new_ext(markdown, options);

    let mut in_table = false;
    let mut in_table_head = false;
    let mut header_row: Vec<String> = Vec::new();
    let mut data_rows: Vec<Vec<String>> = Vec::new();
    let mut current_row: Vec<String> = Vec::new();
    let mut current_cell = String::new();

    for event in parser {
        match event {
            Event::Start(Tag::Table(_)) => {
                in_table = true;
                header_row.clear();
                data_rows.clear();
            }
            Event::End(TagEnd::Table) => {
                in_table = false;
                tables.push((header_row.clone(), data_rows.clone()));
                header_row.clear();
                data_rows.clear();
            }
            Event::Start(Tag::TableHead) => {
                in_table_head = true;
                current_row.clear();
            }
            Event::End(TagEnd::TableHead) => {
                // In pulldown-cmark 0.13+, header cells sit directly inside
                // TableHead with no TableRow wrapper — save them now.
                in_table_head = false;
                header_row = current_row.clone();
                current_row.clear();
            }
            Event::Start(Tag::TableRow) => {
                current_row.clear();
            }
            Event::End(TagEnd::TableRow) => {
                // Only data rows have TableRow wrappers
                data_rows.push(current_row.clone());
                current_row.clear();
            }
            Event::Start(Tag::TableCell) => {
                current_cell.clear();
            }
            Event::End(TagEnd::TableCell) => {
                current_row.push(current_cell.clone());
                current_cell.clear();
            }
            Event::Text(text) if in_table => {
                current_cell.push_str(&text);
            }
            _ => {}
        }
    }

    let _ = in_table_head;
    tables
}

/// Convert Markdown to an XLSX file.
/// GFM tables in the Markdown become worksheets.
/// If no tables are found, writes all lines as plain text to Sheet1.
pub fn markdown_to_xlsx(markdown: &str, path: &str) -> Result<(), ConversionError> {
    let mut workbook = Workbook::new();
    let tables = extract_tables_from_markdown(markdown);

    if tables.is_empty() {
        // Fall back: write plain text lines to Sheet1
        let sheet = workbook
            .add_worksheet()
            .set_name("Sheet1")
            .map_err(|e| ConversionError(format!("Failed to create sheet: {}", e)))?;

        for (row_idx, line) in markdown.lines().enumerate() {
            sheet
                .write_string(row_idx as u32, 0, line)
                .map_err(|e| ConversionError(format!("Failed to write cell: {}", e)))?;
        }
    } else {
        for (table_idx, (header, data_rows)) in tables.iter().enumerate() {
            let sheet_name = format!("Table{}", table_idx + 1);
            let sheet = workbook
                .add_worksheet()
                .set_name(&sheet_name)
                .map_err(|e| ConversionError(format!("Failed to create sheet: {}", e)))?;

            // Write header
            for (col_idx, cell) in header.iter().enumerate() {
                sheet
                    .write_string(0, col_idx as u16, cell)
                    .map_err(|e| ConversionError(format!("Failed to write header: {}", e)))?;
            }

            // Write data rows
            for (row_idx, row) in data_rows.iter().enumerate() {
                for (col_idx, cell) in row.iter().enumerate() {
                    sheet
                        .write_string((row_idx + 1) as u32, col_idx as u16, cell)
                        .map_err(|e| ConversionError(format!("Failed to write data: {}", e)))?;
                }
            }
        }
    }

    workbook
        .save(path)
        .map_err(|e| ConversionError(format!("Failed to save workbook: {}", e)))?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[allow(clippy::approx_constant)]
    fn test_cell_to_string_float() {
        assert_eq!(cell_to_string(&Data::Float(42.0)), "42");
        assert_eq!(cell_to_string(&Data::Float(3.14)), "3.14");
    }

    #[test]
    fn test_cell_to_string_bool() {
        assert_eq!(cell_to_string(&Data::Bool(true)), "TRUE");
        assert_eq!(cell_to_string(&Data::Bool(false)), "FALSE");
    }

    #[test]
    fn test_cell_to_string_escapes_pipe() {
        assert_eq!(cell_to_string(&Data::String("a|b".to_string())), "a\\|b");
    }

    #[test]
    fn test_cell_to_string_collapses_newlines() {
        assert_eq!(
            cell_to_string(&Data::String("line1\nline2".to_string())),
            "line1 line2"
        );
    }

    #[test]
    fn test_excel_serial_to_date() {
        // epoch 1899-12-30; verified with Python: datetime.date(1899,12,30)+timedelta(N)
        assert_eq!(excel_serial_to_date(46078), "2026-02-25");
        assert_eq!(excel_serial_to_date(44927), "2023-01-01");
        // Jan 1, 2000 is a well-known reference point
        assert_eq!(excel_serial_to_date(36526), "2000-01-01");
    }

    #[test]
    fn test_date_col_detection() {
        assert!(is_date_header("Date Reported"));
        assert!(is_date_header("Planned Fix Date"));
        assert!(is_date_header("date"));
        assert!(is_date_header("日期"));
        assert!(!is_date_header("Module"));
        assert!(!is_date_header("Status"));
    }

    #[test]
    fn test_cell_to_string_datetime_variant() {
        use calamine::{ExcelDateTime, ExcelDateTimeType};
        let dt = ExcelDateTime::new(46078.0, ExcelDateTimeType::DateTime, false);
        // In a date column: DateTime → ISO date
        assert_eq!(cell_to_string_ctx(&Data::DateTime(dt.clone()), true), "2026-02-25");
        // Outside a date column: same conversion (Display is serial, but cell_to_string also converts now)
        assert_eq!(cell_to_string(&Data::DateTime(dt)), "2026-02-25");
    }

    #[test]
    fn test_cell_to_string_ctx_date() {
        // Float in a date column → ISO date
        assert_eq!(cell_to_string_ctx(&Data::Float(46078.0), true), "2026-02-25");
        // Int in a date column → ISO date (calamine may return either type)
        assert_eq!(cell_to_string_ctx(&Data::Int(46078), true), "2026-02-25");
        // Same value in a non-date column → raw number
        assert_eq!(cell_to_string_ctx(&Data::Float(46078.0), false), "46078");
        assert_eq!(cell_to_string_ctx(&Data::Int(46078), false), "46078");
    }

    #[test]
    fn test_merge_continuation_rows() {
        let rows = vec![
            vec!["1".into(), "Desc".into(), "".into(), "".into()],
            vec!["".into(), "".into(), "High".into(), "Open".into()],
        ];
        let merged = merge_continuation_rows(rows);
        assert_eq!(merged.len(), 1);
        assert_eq!(merged[0], vec!["1", "Desc", "High", "Open"]);
    }

    #[test]
    fn test_merge_does_not_merge_overlapping_rows() {
        // Both rows have content in column 0 — should NOT merge
        let rows = vec![
            vec!["1".into(), "Desc".into()],
            vec!["Comment".into(), "".into()],
        ];
        let merged = merge_continuation_rows(rows);
        assert_eq!(merged.len(), 2);
    }

    #[test]
    fn test_extract_tables_from_markdown() {
        let md = "| Col1 | Col2 |\n| --- | --- |\n| A | B |\n| C | D |\n";
        let tables = extract_tables_from_markdown(md);
        assert_eq!(tables.len(), 1);
        let (header, data) = &tables[0];
        assert_eq!(header, &["Col1", "Col2"]);
        assert_eq!(data.len(), 2);
        assert_eq!(data[0], &["A", "B"]);
    }

    #[test]
    fn test_extract_tables_no_tables() {
        let md = "# Heading\n\nJust a paragraph.\n";
        let tables = extract_tables_from_markdown(md);
        assert!(tables.is_empty());
    }
}
