// IFR opcode parsing approach inspired by IFRExtractor-RS
// (https://github.com/LongSoft/IFRExtractor-RS)
// Licensed under BSD-2-Clause. Our implementation is a minimal,
// purpose-built subset targeting AMD CBS form navigation only.

use std::collections::{HashMap, HashSet};

use anyhow::{bail, Context};

use crate::hii_question::HiiQuestion;

// HII Package types
const HII_PACKAGE_FORMS: u8 = 0x02;
const HII_PACKAGE_STRINGS: u8 = 0x04;

// IFR Opcodes we care about
const EFI_IFR_FORM_OP: u8 = 0x01;
const EFI_IFR_ONE_OF_OP: u8 = 0x05;
const EFI_IFR_CHECKBOX_OP: u8 = 0x06;
const EFI_IFR_NUMERIC_OP: u8 = 0x07;
const EFI_IFR_SUPPRESS_IF_OP: u8 = 0x0A;
const EFI_IFR_FORM_SET_OP: u8 = 0x0E;
const EFI_IFR_REF_OP: u8 = 0x0F;
const EFI_IFR_END_OP: u8 = 0x29;

/// A single parsed IFR opcode with its raw bytes and metadata.
#[derive(Debug, Clone)]
struct IfrOp {
    opcode: u8,
    has_scope: bool,
    data: Vec<u8>, // everything after the 2-byte header
}

/// Type alias for the split HII package result: (form_packages, string_packages)
type HiiPackageSplit = (Vec<Vec<u8>>, Vec<Vec<u8>>);

/// Type alias for the form index result: (form_id_to_ops_map, root_form_id)
type FormIndex = (HashMap<u16, Vec<IfrOp>>, Option<u16>);

/// Read a u16 little-endian from a byte slice at the given offset.
fn read_u16(data: &[u8], offset: usize) -> Option<u16> {
    if offset + 2 > data.len() {
        return None;
    }
    Some(u16::from_le_bytes([data[offset], data[offset + 1]]))
}

// ---------------------------------------------------------------------------
// HII Package walking
// ---------------------------------------------------------------------------

/// Parse all HII packages from raw HII DB bytes, extracting form and string
/// package payloads.
fn split_hii_packages(hii_db: &[u8]) -> anyhow::Result<HiiPackageSplit> {
    let mut form_packages: Vec<Vec<u8>> = Vec::new();
    let mut string_packages: Vec<Vec<u8>> = Vec::new();
    let mut offset = 0;

    while offset + 4 <= hii_db.len() {
        let header = u32::from_le_bytes([
            hii_db[offset],
            hii_db[offset + 1],
            hii_db[offset + 2],
            hii_db[offset + 3],
        ]);
        let length = (header & 0x00FF_FFFF) as usize;
        let pkg_type = (header >> 24) as u8;

        if length < 4 {
            // End-of-packages marker or invalid — stop
            break;
        }
        if offset + length > hii_db.len() {
            bail!(
                "HII package at offset {} claims length {} but only {} bytes remain",
                offset,
                length,
                hii_db.len() - offset
            );
        }

        let payload = &hii_db[offset + 4..offset + length];
        match pkg_type {
            HII_PACKAGE_FORMS => form_packages.push(payload.to_vec()),
            HII_PACKAGE_STRINGS => string_packages.push(payload.to_vec()),
            _ => { /* skip other package types */ }
        }

        offset += length;
    }

    Ok((form_packages, string_packages))
}

// ---------------------------------------------------------------------------
// String table construction
// ---------------------------------------------------------------------------

/// Parse a UCS-2 / SCSU string package into a map of StringId → String.
///
/// String packages have a fixed-size header (HdrSize field at offset 0, typically 0x34 = 52 bytes)
/// followed by SIBT (String Information Block Type) records.
/// We handle the most common block types: SCSU (0x10) and UCS2 (0x14), plus
/// Skip1 (0x22) and End (0x00).
fn parse_string_package(data: &[u8]) -> HashMap<u16, String> {
    let mut strings = HashMap::new();

    if data.len() < 4 {
        return strings;
    }

    // Header size is the first u32 in the string package data (after the 4-byte pkg header).
    let hdr_size = u32::from_le_bytes([
        data[0],
        data.get(1).copied().unwrap_or(0),
        data.get(2).copied().unwrap_or(0),
        data.get(3).copied().unwrap_or(0),
    ]) as usize;

    if hdr_size < 4 || hdr_size > data.len() {
        return strings;
    }

    // HdrSize counts from pkg start; we stripped the 4-byte pkg header already
    let mut pos = hdr_size - 4;
    let mut string_id: u16 = 1; // 1-based

    while pos < data.len() {
        let block_type = data[pos];
        pos += 1;

        match block_type {
            0x00 => break, // SIBT_END
            0x10 => {
                // SIBT_STRING_SCSU — null-terminated SCSU (effectively ASCII/Latin-1)
                let start = pos;
                while pos < data.len() && data[pos] != 0 {
                    pos += 1;
                }
                let s = String::from_utf8_lossy(&data[start..pos]).to_string();
                strings.insert(string_id, s);
                string_id += 1;
                if pos < data.len() {
                    pos += 1; // skip null terminator
                }
            }
            0x14 => {
                // SIBT_STRING_UCS2 — null-terminated UTF-16LE
                let start = pos;
                let mut chars: Vec<u16> = Vec::new();
                while pos + 1 < data.len() {
                    let ch = u16::from_le_bytes([data[pos], data[pos + 1]]);
                    pos += 2;
                    if ch == 0 {
                        break;
                    }
                    chars.push(ch);
                }
                let s = String::from_utf16_lossy(&chars);
                let _ = start; // suppress unused warning
                strings.insert(string_id, s);
                string_id += 1;
            }
            0x21 => {
                // SIBT_SKIP2 — skip count (u16) string IDs
                if pos + 2 <= data.len() {
                    let count = u16::from_le_bytes([data[pos], data[pos + 1]]);
                    string_id += count;
                    pos += 2;
                } else {
                    break;
                }
            }
            0x22 => {
                // SIBT_SKIP1 — skip count (u8) string IDs
                if pos < data.len() {
                    string_id += data[pos] as u16;
                    pos += 1;
                } else {
                    break;
                }
            }
            _ => {
                // Unknown SIBT block type — we cannot determine its length safely,
                // so stop parsing this string package.
                tracing::debug!(
                    block_type = block_type,
                    offset = pos - 1,
                    "unknown SIBT block type in string package, stopping parse"
                );
                break;
            }
        }
    }

    strings
}

/// Build a merged string table from all string packages.
fn build_string_table(string_packages: &[Vec<u8>]) -> HashMap<u16, String> {
    let mut table = HashMap::new();
    for pkg in string_packages {
        let partial = parse_string_package(pkg);
        // Later packages can override earlier ones (language variants).
        for (id, s) in partial {
            table.insert(id, s);
        }
    }
    table
}

/// Resolve a StringId from the table, falling back to a placeholder.
fn resolve_string(table: &HashMap<u16, String>, id: u16) -> String {
    if id == 0 {
        return String::new();
    }
    table
        .get(&id)
        .cloned()
        .unwrap_or_else(|| format!("<string {}>", id))
}

// ---------------------------------------------------------------------------
// IFR opcode parsing — first pass
// ---------------------------------------------------------------------------

/// Walk the raw IFR opcode stream in a form package, building a map of
/// FormId → Vec<IfrOp>.
///
/// We also identify the root FormId (the first FORM_OP encountered).
fn build_form_index(form_data: &[u8]) -> anyhow::Result<FormIndex> {
    let mut form_index: HashMap<u16, Vec<IfrOp>> = HashMap::new();
    let mut current_form_id: Option<u16> = None;
    let mut root_form_id: Option<u16> = None;
    let mut offset = 0;

    // We need to track scope depth so we know when a FORM scope ends
    // and we should stop attributing opcodes to that form.
    // However, forms are typically sequential in a formset, not nested.
    // Simplified approach: collect opcodes per form linearly.

    while offset + 2 <= form_data.len() {
        let opcode = form_data[offset];
        let len_byte = form_data[offset + 1];
        let length = (len_byte & 0x7F) as usize;
        let has_scope = (len_byte & 0x80) != 0;

        // Guard against zero-length opcodes (prevent infinite loop)
        let advance = if length < 2 && opcode != EFI_IFR_END_OP {
            2 // minimum advance
        } else {
            length
        };

        if offset + advance > form_data.len() {
            bail!(
                "IFR opcode 0x{:02X} at offset {} is truncated: need {} bytes but only {} remain",
                opcode,
                offset,
                advance,
                form_data.len() - offset
            );
        }

        let data = if length > 2 {
            form_data[offset + 2..offset + length].to_vec()
        } else {
            Vec::new()
        };

        let op = IfrOp {
            opcode,
            has_scope,
            data,
        };

        match opcode {
            EFI_IFR_FORM_OP => {
                if let Some(form_id) = read_u16(&op.data, 0) {
                    current_form_id = Some(form_id);
                    if root_form_id.is_none() {
                        root_form_id = Some(form_id);
                    }
                    form_index.entry(form_id).or_default();
                }
            }
            EFI_IFR_FORM_SET_OP => {
                // FormSet is a container — ignore its data for now but don't
                // assign it to any form.
            }
            _ => {
                if let Some(fid) = current_form_id {
                    form_index.entry(fid).or_default().push(op);
                }
            }
        }

        offset += advance;
    }

    Ok((form_index, root_form_id))
}

// ---------------------------------------------------------------------------
// Second pass — recursive form walking
// ---------------------------------------------------------------------------

/// Extract a HiiQuestion from a question-type opcode (OneOf, Numeric, CheckBox).
///
/// The question header layout (data bytes, i.e. after the 2-byte opcode header):
///   [0..2] PromptStringId (u16)
///   [2..4] HelpStringId   (u16)
///   [4..6] QuestionId     (u16)
///   [6..8] VarStoreId     (u16)
///   [8..10] VarStoreInfo  (u16) — offset into VarStore
///   [10]   QuestionFlags  (u8)
fn extract_question(op: &IfrOp, string_table: &HashMap<u16, String>) -> Option<HiiQuestion> {
    let prompt_id = read_u16(&op.data, 0)?;
    let help_id = read_u16(&op.data, 2)?;

    let name = resolve_string(string_table, prompt_id);
    let help = resolve_string(string_table, help_id);

    // We cannot read the actual VarStore value without runtime access to
    // /sys/firmware/efi/efivars, so answer is left empty for now.
    // The downstream parse_hii_questions() handles empty answers gracefully.
    Some(HiiQuestion {
        name,
        answer: String::new(),
        help,
    })
}

/// Extract the target FormId from a REF_OP.
///
/// REF_OP data layout (after 2-byte opcode header):
///   [0..2]  PromptStringId  (u16)
///   [2..4]  HelpStringId    (u16)
///   [4..6]  QuestionId      (u16)
///   [6..8]  VarStoreId      (u16)
///   [8..10] VarStoreInfo    (u16)
///   [10]    QuestionFlags   (u8)
///   [11..13] FormId         (u16)  — the target form to navigate to
fn extract_ref_form_id(op: &IfrOp) -> Option<u16> {
    read_u16(&op.data, 11)
}

/// Recursively walk a form and its sub-forms (via REF_OP), collecting questions.
fn walk_form(
    form_id: u16,
    form_index: &HashMap<u16, Vec<IfrOp>>,
    string_table: &HashMap<u16, String>,
    visited: &mut HashSet<u16>,
) -> Vec<HiiQuestion> {
    if !visited.insert(form_id) {
        // Already visited — cycle guard
        return Vec::new();
    }

    let ops = match form_index.get(&form_id) {
        Some(ops) => ops,
        None => return Vec::new(),
    };

    let mut questions = Vec::new();
    let mut i = 0;

    while i < ops.len() {
        let op = &ops[i];

        match op.opcode {
            EFI_IFR_SUPPRESS_IF_OP => {
                // Skip all opcodes inside this SUPPRESS_IF scope until matching END_OP.
                if op.has_scope {
                    let mut depth = 1u32;
                    i += 1;
                    while i < ops.len() && depth > 0 {
                        if ops[i].has_scope {
                            depth += 1;
                        }
                        if ops[i].opcode == EFI_IFR_END_OP {
                            depth = depth.saturating_sub(1);
                        }
                        i += 1;
                    }
                    continue;
                }
            }
            EFI_IFR_REF_OP => {
                if let Some(target_form_id) = extract_ref_form_id(op) {
                    let sub_questions =
                        walk_form(target_form_id, form_index, string_table, visited);
                    questions.extend(sub_questions);
                }
            }
            EFI_IFR_ONE_OF_OP | EFI_IFR_NUMERIC_OP | EFI_IFR_CHECKBOX_OP => {
                if let Some(q) = extract_question(op, string_table) {
                    questions.push(q);
                }
            }
            _ => { /* skip subtitles, text, end, varstore, etc. */ }
        }

        i += 1;
    }

    questions
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Parse an HII database binary blob into a list of HiiQuestions.
///
/// This performs a 2-pass parse:
/// 1. Split packages, build string table, index forms by FormId
/// 2. Walk root form recursively following REF_OP links into sub-forms
pub fn parse_ifr_to_questions(hii_db: &[u8]) -> anyhow::Result<Vec<HiiQuestion>> {
    if hii_db.len() < 4 {
        bail!(
            "HII database too short ({} bytes): need at least 4 bytes for a package header",
            hii_db.len()
        );
    }

    let (form_packages, string_packages) =
        split_hii_packages(hii_db).context("failed to split HII packages")?;

    let string_table = build_string_table(&string_packages);

    let mut all_questions = Vec::new();

    for form_pkg in &form_packages {
        let (form_index, root_form_id) =
            build_form_index(form_pkg).context("failed to build form index")?;

        if let Some(root_id) = root_form_id {
            // Walk ALL forms, starting from root and following REFs
            let mut visited = HashSet::new();
            let questions = walk_form(root_id, &form_index, &string_table, &mut visited);
            all_questions.extend(questions);

            // Also walk any forms not reachable from root (orphan forms)
            for &fid in form_index.keys() {
                if !visited.contains(&fid) {
                    let orphan_questions = walk_form(fid, &form_index, &string_table, &mut visited);
                    all_questions.extend(orphan_questions);
                }
            }
        }
    }

    tracing::debug!(
        question_count = all_questions.len(),
        form_packages = form_packages.len(),
        string_packages = string_packages.len(),
        "IFR parsing complete"
    );

    Ok(all_questions)
}

// ---------------------------------------------------------------------------
// Test helpers for building synthetic HII DB bytes
// ---------------------------------------------------------------------------
#[cfg(test)]
mod test_helpers {
    /// Build a HII package header (4 bytes): length in bits 0-23, type in bits 24-31.
    pub fn make_pkg_header(length: u32, pkg_type: u8) -> Vec<u8> {
        let header = (length & 0x00FF_FFFF) | ((pkg_type as u32) << 24);
        header.to_le_bytes().to_vec()
    }

    /// Build an IFR opcode header (2 bytes).
    pub fn make_opcode_header(opcode: u8, length: u8, has_scope: bool) -> Vec<u8> {
        let len_byte = if has_scope { length | 0x80 } else { length };
        vec![opcode, len_byte]
    }

    /// Build a minimal FORM_OP: opcode(0x01) + length(6) + scope + FormId(u16) + TitleStringId(u16).
    pub fn make_form_op(form_id: u16, title_string_id: u16) -> Vec<u8> {
        let mut bytes = make_opcode_header(0x01, 6, true); // FORM always has scope
        bytes.extend_from_slice(&form_id.to_le_bytes());
        bytes.extend_from_slice(&title_string_id.to_le_bytes());
        bytes
    }

    /// Build a NUMERIC_OP with a question header.
    /// Layout: opcode(0x07) + len + PromptStringId + HelpStringId + QuestionId +
    ///         VarStoreId + VarStoreInfo + QuestionFlags + Flags + min/max/step (u8×3)
    pub fn make_numeric_op(prompt_string_id: u16, help_string_id: u16) -> Vec<u8> {
        let total_len: u8 = 2 + 2 + 2 + 2 + 2 + 2 + 1 + 1 + 3; // = 17
        let mut bytes = make_opcode_header(0x07, total_len, false);
        bytes.extend_from_slice(&prompt_string_id.to_le_bytes()); // PromptStringId
        bytes.extend_from_slice(&help_string_id.to_le_bytes()); // HelpStringId
        bytes.extend_from_slice(&1u16.to_le_bytes()); // QuestionId
        bytes.extend_from_slice(&1u16.to_le_bytes()); // VarStoreId
        bytes.extend_from_slice(&0u16.to_le_bytes()); // VarStoreInfo
        bytes.push(0x00); // QuestionFlags
        bytes.push(0x00); // Flags (NumSize8)
        bytes.extend_from_slice(&[0, 255, 1]); // min, max, step
        bytes
    }

    /// Build a ONE_OF_OP with a question header.
    pub fn make_oneof_op(prompt_string_id: u16, help_string_id: u16) -> Vec<u8> {
        let total_len: u8 = 2 + 2 + 2 + 2 + 2 + 2 + 1 + 1 + 3; // = 17
        let mut bytes = make_opcode_header(0x05, total_len, true); // OneOf has scope
        bytes.extend_from_slice(&prompt_string_id.to_le_bytes());
        bytes.extend_from_slice(&help_string_id.to_le_bytes());
        bytes.extend_from_slice(&1u16.to_le_bytes()); // QuestionId
        bytes.extend_from_slice(&1u16.to_le_bytes()); // VarStoreId
        bytes.extend_from_slice(&0u16.to_le_bytes()); // VarStoreInfo
        bytes.push(0x00); // QuestionFlags
        bytes.push(0x00); // Flags (NumSize8)
        bytes.extend_from_slice(&[0, 255, 1]); // min, max, step
        bytes
    }

    /// Build a REF_OP pointing to a target FormId.
    /// Layout: opcode(0x0F) + len + QuestionHeader(11 bytes) + FormId(2 bytes)
    pub fn make_ref_op(target_form_id: u16) -> Vec<u8> {
        let total_len: u8 = 2 + 11 + 2; // = 15
        let mut bytes = make_opcode_header(0x0F, total_len, false);
        bytes.extend_from_slice(&0u16.to_le_bytes()); // PromptStringId
        bytes.extend_from_slice(&0u16.to_le_bytes()); // HelpStringId
        bytes.extend_from_slice(&0u16.to_le_bytes()); // QuestionId
        bytes.extend_from_slice(&0u16.to_le_bytes()); // VarStoreId
        bytes.extend_from_slice(&0u16.to_le_bytes()); // VarStoreInfo
        bytes.push(0x00); // QuestionFlags
        bytes.extend_from_slice(&target_form_id.to_le_bytes()); // target FormId
        bytes
    }

    /// Build a SUPPRESS_IF_OP (scope opener).
    pub fn make_suppress_if_op() -> Vec<u8> {
        make_opcode_header(0x0A, 2, true) // SUPPRESS_IF has scope
    }

    /// Build a FORMSET_OP (minimal: just GUID + title + help + flags).
    pub fn make_formset_op() -> Vec<u8> {
        // header(2) + GUID(16) + TitleStringId(2) + HelpStringId(2) + Flags(1) = 23
        let total_len: u8 = 2 + 16 + 2 + 2 + 1;
        let mut bytes = make_opcode_header(0x0E, total_len, true);
        bytes.extend_from_slice(&[0u8; 16]);
        bytes.extend_from_slice(&0u16.to_le_bytes());
        bytes.extend_from_slice(&0u16.to_le_bytes());
        bytes.push(0x00);
        bytes
    }

    /// Build an END_OP.
    pub fn make_end_op() -> Vec<u8> {
        make_opcode_header(0x29, 2, false)
    }

    /// Build a minimal string package with SCSU strings.
    /// Returns the package payload (without the 4-byte package header).
    pub fn make_string_package(strings: &[&str]) -> Vec<u8> {
        let hdr_size: u32 = 0x34; // Standard string package header size

        let mut data = Vec::new();
        data.extend_from_slice(&hdr_size.to_le_bytes());
        data.extend_from_slice(&hdr_size.to_le_bytes());
        data.extend_from_slice(&[0u8; 32]);
        data.extend_from_slice(&0u16.to_le_bytes());
        let lang = b"en\0";
        data.extend_from_slice(lang);
        // Pad to (hdr_size - 4) to account for stripped pkg header
        while data.len() < (hdr_size as usize - 4) {
            data.push(0);
        }

        for s in strings {
            data.push(0x10);
            data.extend_from_slice(s.as_bytes());
            data.push(0x00);
        }
        data.push(0x00);

        data
    }

    /// Build a complete minimal HII DB with form and optional string packages.
    pub fn make_hii_db(form_data: &[u8], string_data: Option<&[u8]>) -> Vec<u8> {
        let mut db = Vec::new();

        // Form package
        let form_pkg_len = 4 + form_data.len();
        db.extend_from_slice(&make_pkg_header(form_pkg_len as u32, 0x02));
        db.extend_from_slice(form_data);

        // String package (optional)
        if let Some(sdata) = string_data {
            let str_pkg_len = 4 + sdata.len();
            db.extend_from_slice(&make_pkg_header(str_pkg_len as u32, 0x04));
            db.extend_from_slice(sdata);
        }

        db
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use test_helpers::*;

    #[test]
    fn given_minimal_formset_bytes_when_parsing_then_returns_questions() {
        // Build: FormSet + Form(id=1) + Numeric(prompt=1, help=2) + End + End
        let mut form_data = Vec::new();
        form_data.extend_from_slice(&make_formset_op());
        form_data.extend_from_slice(&make_form_op(1, 0));
        form_data.extend_from_slice(&make_numeric_op(1, 2));
        form_data.extend_from_slice(&make_end_op()); // end Form scope
        form_data.extend_from_slice(&make_end_op()); // end FormSet scope

        let string_data = make_string_package(&["Test Question", "Help Text"]);
        let hii_db = make_hii_db(&form_data, Some(&string_data));

        let questions =
            parse_ifr_to_questions(&hii_db).expect("should parse minimal formset successfully");

        assert_eq!(questions.len(), 1);
        assert_eq!(questions[0].name, "Test Question");
        assert_eq!(questions[0].help, "Help Text");
        assert_eq!(questions[0].answer, "");
    }

    #[test]
    fn given_form_with_ref_to_subform_when_parsing_then_follows_ref() {
        // Root Form(id=1): contains REF_OP → FormId=2
        // Sub Form(id=2): contains Numeric "CO Offset Core 0"
        let mut form_data = Vec::new();
        form_data.extend_from_slice(&make_formset_op());

        // Form 1: root with a REF to form 2
        form_data.extend_from_slice(&make_form_op(1, 0));
        form_data.extend_from_slice(&make_ref_op(2));
        form_data.extend_from_slice(&make_end_op()); // end Form 1

        // Form 2: sub-form with a numeric question
        form_data.extend_from_slice(&make_form_op(2, 0));
        form_data.extend_from_slice(&make_numeric_op(1, 2));
        form_data.extend_from_slice(&make_end_op()); // end Form 2

        form_data.extend_from_slice(&make_end_op()); // end FormSet

        let string_data = make_string_package(&["CO Offset Core 0", "Curve optimizer offset"]);
        let hii_db = make_hii_db(&form_data, Some(&string_data));

        let questions =
            parse_ifr_to_questions(&hii_db).expect("should follow REF_OP into sub-form");

        assert!(
            questions.iter().any(|q| q.name == "CO Offset Core 0"),
            "should find question from sub-form via REF_OP traversal, got: {:?}",
            questions
        );
    }

    #[test]
    fn given_oneof_question_when_parsing_then_extracts_name() {
        let mut form_data = Vec::new();
        form_data.extend_from_slice(&make_formset_op());
        form_data.extend_from_slice(&make_form_op(1, 0));
        form_data.extend_from_slice(&make_oneof_op(1, 2));
        form_data.extend_from_slice(&make_end_op()); // end OneOf scope
        form_data.extend_from_slice(&make_end_op()); // end Form scope
        form_data.extend_from_slice(&make_end_op()); // end FormSet scope

        let string_data = make_string_package(&["Precision Boost Overdrive", "PBO help text"]);
        let hii_db = make_hii_db(&form_data, Some(&string_data));

        let questions = parse_ifr_to_questions(&hii_db).expect("should parse OneOf question");

        assert_eq!(questions.len(), 1);
        assert_eq!(questions[0].name, "Precision Boost Overdrive");
    }

    #[test]
    fn given_numeric_question_when_parsing_then_extracts_name() {
        let mut form_data = Vec::new();
        form_data.extend_from_slice(&make_formset_op());
        form_data.extend_from_slice(&make_form_op(1, 0));
        form_data.extend_from_slice(&make_numeric_op(1, 2));
        form_data.extend_from_slice(&make_end_op()); // end Form
        form_data.extend_from_slice(&make_end_op()); // end FormSet

        let string_data = make_string_package(&["PPT Limit", "Platform power limit"]);
        let hii_db = make_hii_db(&form_data, Some(&string_data));

        let questions = parse_ifr_to_questions(&hii_db).expect("should parse Numeric question");

        assert_eq!(questions.len(), 1);
        assert_eq!(questions[0].name, "PPT Limit");
        assert_eq!(questions[0].help, "Platform power limit");
    }

    #[test]
    fn given_truncated_bytes_when_parsing_then_returns_error() {
        let truncated = vec![0x01, 0x02, 0x03]; // too short for any valid package

        let result = parse_ifr_to_questions(&truncated);

        assert!(
            result.is_err(),
            "should return error for truncated input, got: {:?}",
            result
        );
    }

    #[test]
    fn given_zero_length_opcode_when_parsing_then_skips_safely() {
        // Build form data with a zero-length opcode that is NOT END_OP
        let mut form_data = Vec::new();
        form_data.extend_from_slice(&make_formset_op());
        form_data.extend_from_slice(&make_form_op(1, 0));
        // Insert a bogus opcode with length=0 (byte 0xFF, len=0)
        form_data.push(0xFF); // unknown opcode
        form_data.push(0x00); // length = 0
        form_data.extend_from_slice(&make_end_op()); // end Form
        form_data.extend_from_slice(&make_end_op()); // end FormSet

        let hii_db = make_hii_db(&form_data, None);

        let result =
            parse_ifr_to_questions(&hii_db).expect("should handle zero-length opcode gracefully");

        // No questions expected — just verify it doesn't infinite loop or error
        assert!(
            result.is_empty(),
            "should return empty list for form with only a zero-length opcode"
        );
    }

    #[test]
    fn given_unknown_opcode_when_parsing_then_skips_gracefully() {
        // Build form with: unknown opcode (0xFE, len=4, 2 data bytes) + numeric question
        let mut form_data = Vec::new();
        form_data.extend_from_slice(&make_formset_op());
        form_data.extend_from_slice(&make_form_op(1, 0));
        // Unknown opcode 0xFE with length=4 (2 header + 2 data)
        form_data.push(0xFE);
        form_data.push(0x04); // length=4
        form_data.push(0xAA); // dummy data
        form_data.push(0xBB); // dummy data
                              // Then a real numeric question
        form_data.extend_from_slice(&make_numeric_op(1, 0));
        form_data.extend_from_slice(&make_end_op()); // end Form
        form_data.extend_from_slice(&make_end_op()); // end FormSet

        let string_data = make_string_package(&["Real Question"]);
        let hii_db = make_hii_db(&form_data, Some(&string_data));

        let questions = parse_ifr_to_questions(&hii_db)
            .expect("should skip unknown opcode and process remaining");

        assert_eq!(questions.len(), 1);
        assert_eq!(questions[0].name, "Real Question");
    }

    #[test]
    fn given_string_package_when_parsing_then_resolves_string_ids() {
        let mut form_data = Vec::new();
        form_data.extend_from_slice(&make_formset_op());
        form_data.extend_from_slice(&make_form_op(1, 0));
        // Numeric referencing StringId=1 for name, StringId=2 for help
        form_data.extend_from_slice(&make_numeric_op(1, 2));
        form_data.extend_from_slice(&make_end_op()); // end Form
        form_data.extend_from_slice(&make_end_op()); // end FormSet

        let string_data =
            make_string_package(&["Core 0 Curve Optimizer Offset", "Adjust CO for core 0"]);
        let hii_db = make_hii_db(&form_data, Some(&string_data));

        let questions = parse_ifr_to_questions(&hii_db).expect("should resolve string IDs");

        assert_eq!(questions.len(), 1);
        assert_eq!(questions[0].name, "Core 0 Curve Optimizer Offset");
        assert_eq!(questions[0].help, "Adjust CO for core 0");
    }

    #[test]
    fn given_suppress_if_scope_when_parsing_then_skips_suppressed_questions() {
        let mut form_data = Vec::new();
        form_data.extend_from_slice(&make_formset_op());
        form_data.extend_from_slice(&make_form_op(1, 0));

        // A visible numeric question
        form_data.extend_from_slice(&make_numeric_op(1, 0));

        // SUPPRESS_IF scope containing a hidden question
        form_data.extend_from_slice(&make_suppress_if_op());
        form_data.extend_from_slice(&make_numeric_op(2, 0));
        form_data.extend_from_slice(&make_end_op()); // end SUPPRESS_IF

        form_data.extend_from_slice(&make_end_op()); // end Form
        form_data.extend_from_slice(&make_end_op()); // end FormSet

        let string_data = make_string_package(&["Visible Question", "Hidden Question"]);
        let hii_db = make_hii_db(&form_data, Some(&string_data));

        let questions = parse_ifr_to_questions(&hii_db).expect("should handle SUPPRESS_IF scope");

        assert_eq!(
            questions.len(),
            1,
            "only the visible question should be returned"
        );
        assert_eq!(questions[0].name, "Visible Question");
    }
}
