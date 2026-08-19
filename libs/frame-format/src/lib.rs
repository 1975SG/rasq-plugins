//! frame/packet builder and inspector -- the half of `docs/roadmap/
//! workbenches.md`'s Protocol/systems analysis tooling category
//! `checksums/`'s own doc comment named as "a separate, larger design
//! pass (what does 'a frame' mean generically enough to build, per
//! ADR-011)". Answer: a declared, ordered list of fixed-width fields
//! (numeric with explicit endianness, or a raw byte run) plus a
//! trailing checksum computed over them -- generic enough to describe a
//! real wire-format header without hardcoding to any one protocol.

use checksums::{crc16_modbus, crc16_xmodem, crc32, internet_checksum};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FieldKind {
    U8,
    U16Le,
    U16Be,
    U32Le,
    U32Be,
    Bytes(usize),
}

impl FieldKind {
    pub fn byte_width(self) -> usize {
        match self {
            FieldKind::U8 => 1,
            FieldKind::U16Le | FieldKind::U16Be => 2,
            FieldKind::U32Le | FieldKind::U32Be => 4,
            FieldKind::Bytes(n) => n,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FieldSpec {
    pub name: String,
    pub kind: FieldKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChecksumKind {
    Crc32,
    Crc16Xmodem,
    Crc16Modbus,
    InternetChecksum,
}

impl ChecksumKind {
    pub fn trailer_width(self) -> usize {
        match self {
            ChecksumKind::Crc32 => 4,
            ChecksumKind::Crc16Xmodem | ChecksumKind::Crc16Modbus | ChecksumKind::InternetChecksum => 2,
        }
    }

    /// trailer bytes, big-endian on the wire -- the conventional byte
    /// order for a trailing check value regardless of the checksum's
    /// own internal bit-reflection (matches `example-view-checksum-
    /// inspector`'s own `to_be_bytes()` convention for its CRC-32
    /// trailer).
    pub fn compute(self, payload: &[u8]) -> Vec<u8> {
        match self {
            ChecksumKind::Crc32 => crc32(payload).to_be_bytes().to_vec(),
            ChecksumKind::Crc16Xmodem => crc16_xmodem(payload).to_be_bytes().to_vec(),
            ChecksumKind::Crc16Modbus => crc16_modbus(payload).to_be_bytes().to_vec(),
            ChecksumKind::InternetChecksum => internet_checksum(payload).to_be_bytes().to_vec(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FrameSpec {
    pub fields: Vec<FieldSpec>,
    pub checksum: ChecksumKind,
}

impl FrameSpec {
    pub fn payload_width(&self) -> usize {
        self.fields.iter().map(|f| f.kind.byte_width()).sum()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FieldValue {
    Number(u64),
    Bytes(Vec<u8>),
}

fn pack_field(kind: FieldKind, value: &FieldValue, out: &mut Vec<u8>) -> Result<(), String> {
    match (kind, value) {
        (FieldKind::U8, FieldValue::Number(n)) => {
            out.push((*n).try_into().map_err(|_| format!("{n} doesn't fit in u8"))?);
        }
        (FieldKind::U16Le, FieldValue::Number(n)) => {
            let v: u16 = (*n).try_into().map_err(|_| format!("{n} doesn't fit in u16"))?;
            out.extend_from_slice(&v.to_le_bytes());
        }
        (FieldKind::U16Be, FieldValue::Number(n)) => {
            let v: u16 = (*n).try_into().map_err(|_| format!("{n} doesn't fit in u16"))?;
            out.extend_from_slice(&v.to_be_bytes());
        }
        (FieldKind::U32Le, FieldValue::Number(n)) => {
            let v: u32 = (*n).try_into().map_err(|_| format!("{n} doesn't fit in u32"))?;
            out.extend_from_slice(&v.to_le_bytes());
        }
        (FieldKind::U32Be, FieldValue::Number(n)) => {
            let v: u32 = (*n).try_into().map_err(|_| format!("{n} doesn't fit in u32"))?;
            out.extend_from_slice(&v.to_be_bytes());
        }
        (FieldKind::Bytes(width), FieldValue::Bytes(bytes)) => {
            if bytes.len() != width {
                return Err(format!("expected {width} byte(s), got {}", bytes.len()));
            }
            out.extend_from_slice(bytes);
        }
        (kind, value) => return Err(format!("field kind {kind:?} can't hold value {value:?}")),
    }
    Ok(())
}

fn unpack_field(kind: FieldKind, bytes: &[u8]) -> FieldValue {
    match kind {
        FieldKind::U8 => FieldValue::Number(bytes[0] as u64),
        FieldKind::U16Le => FieldValue::Number(u16::from_le_bytes([bytes[0], bytes[1]]) as u64),
        FieldKind::U16Be => FieldValue::Number(u16::from_be_bytes([bytes[0], bytes[1]]) as u64),
        FieldKind::U32Le => FieldValue::Number(u32::from_le_bytes(bytes.try_into().unwrap()) as u64),
        FieldKind::U32Be => FieldValue::Number(u32::from_be_bytes(bytes.try_into().unwrap()) as u64),
        FieldKind::Bytes(_) => FieldValue::Bytes(bytes.to_vec()),
    }
}

/// packs `values` (one per `spec.fields`, same order) into wire bytes
/// and appends the spec's own checksum trailer, computed over the
/// packed payload -- the "builder" half.
pub fn build_frame(spec: &FrameSpec, values: &[FieldValue]) -> Result<Vec<u8>, String> {
    if values.len() != spec.fields.len() {
        return Err(format!("expected {} field value(s), got {}", spec.fields.len(), values.len()));
    }
    let mut payload = Vec::with_capacity(spec.payload_width());
    for (field, value) in spec.fields.iter().zip(values) {
        pack_field(field.kind, value, &mut payload).map_err(|e| format!("field {:?}: {e}", field.name))?;
    }
    let trailer = spec.checksum.compute(&payload);
    payload.extend_from_slice(&trailer);
    Ok(payload)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InspectedFrame {
    pub fields: Vec<(String, FieldValue)>,
    pub checksum_ok: bool,
    pub expected_trailer: Vec<u8>,
    pub actual_trailer: Vec<u8>,
}

/// unpacks raw wire `bytes` per `spec` and verifies the trailing
/// checksum -- the "inspector" half. Errors only on a structural
/// mismatch (wrong total length); a checksum mismatch is reported in
/// `checksum_ok`, not an `Err` -- a corrupted-but-well-formed frame is
/// exactly the case an inspector exists to catch and report, not
/// refuse to look at.
pub fn inspect_frame(spec: &FrameSpec, bytes: &[u8]) -> Result<InspectedFrame, String> {
    let expected_len = spec.payload_width() + spec.checksum.trailer_width();
    if bytes.len() != expected_len {
        return Err(format!("expected {expected_len} byte(s), got {}", bytes.len()));
    }
    let (payload, actual_trailer) = bytes.split_at(spec.payload_width());

    let mut fields = Vec::with_capacity(spec.fields.len());
    let mut offset = 0;
    for field in &spec.fields {
        let width = field.kind.byte_width();
        fields.push((field.name.clone(), unpack_field(field.kind, &payload[offset..offset + width])));
        offset += width;
    }

    let expected_trailer = spec.checksum.compute(payload);
    Ok(InspectedFrame {
        fields,
        checksum_ok: expected_trailer == actual_trailer,
        expected_trailer,
        actual_trailer: actual_trailer.to_vec(),
    })
}

fn parse_kind(s: &str) -> Result<FieldKind, String> {
    match s {
        "u8" => Ok(FieldKind::U8),
        "u16le" => Ok(FieldKind::U16Le),
        "u16be" => Ok(FieldKind::U16Be),
        "u32le" => Ok(FieldKind::U32Le),
        "u32be" => Ok(FieldKind::U32Be),
        other => match other.strip_prefix("bytes") {
            Some(n) => Ok(FieldKind::Bytes(n.parse().map_err(|_| format!("invalid field kind {other:?}"))?)),
            None => Err(format!("unknown field kind {other:?}")),
        },
    }
}

fn parse_checksum_kind(s: &str) -> Result<ChecksumKind, String> {
    match s {
        "crc32" => Ok(ChecksumKind::Crc32),
        "crc16-xmodem" => Ok(ChecksumKind::Crc16Xmodem),
        "crc16-modbus" => Ok(ChecksumKind::Crc16Modbus),
        "internet-checksum" => Ok(ChecksumKind::InternetChecksum),
        other => Err(format!("unknown checksum kind {other:?}")),
    }
}

fn parse_field_value(kind: FieldKind, raw: &str) -> Result<FieldValue, String> {
    match kind {
        FieldKind::Bytes(width) => {
            if raw.len() != width * 2 {
                return Err(format!("expected {} hex chars for a {width}-byte field, got {}", width * 2, raw.len()));
            }
            let mut bytes = Vec::with_capacity(width);
            for i in (0..raw.len()).step_by(2) {
                bytes.push(
                    u8::from_str_radix(&raw[i..i + 2], 16).map_err(|_| format!("invalid hex byte {:?}", &raw[i..i + 2]))?,
                );
            }
            Ok(FieldValue::Bytes(bytes))
        }
        _ => {
            let n = match raw.strip_prefix("0x") {
                Some(hex) => u64::from_str_radix(hex, 16).map_err(|_| format!("invalid hex number {raw:?}"))?,
                None => raw.parse::<u64>().map_err(|_| format!("invalid number {raw:?}"))?,
            };
            Ok(FieldValue::Number(n))
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeclaredFrame {
    pub label: String,
    pub values: Vec<FieldValue>,
}

fn finish_frame(label: String, pairs: &[(String, String)], fields: &[FieldSpec]) -> Result<DeclaredFrame, String> {
    let mut values = Vec::with_capacity(fields.len());
    for field in fields {
        let raw = pairs
            .iter()
            .find(|(k, _)| k == &field.name)
            .map(|(_, v)| v.as_str())
            .ok_or_else(|| format!("frame {label:?}: missing value for field {:?}", field.name))?;
        values.push(
            parse_field_value(field.kind, raw)
                .map_err(|e| format!("frame {label:?}, field {:?}: {e}", field.name))?,
        );
    }
    Ok(DeclaredFrame { label, values })
}

/// `field,<kind>,<name>` lines (declared before any `frame,` block),
/// then one `checksum,<kind>` line, then any number of `frame,<label>`
/// blocks each followed by `field_name=value` lines -- numeric fields
/// as decimal or `0x`-prefixed hex, `bytesN` fields as exactly `2*N`
/// hex characters. `#` comments and blank lines skipped throughout.
/// same "one crate owns a format's math and its parsing" shape every
/// prior capture-format crate this project has already established.
pub fn parse_frame_format(text: &str) -> Result<(FrameSpec, Vec<DeclaredFrame>), String> {
    let mut fields: Vec<FieldSpec> = Vec::new();
    let mut checksum: Option<ChecksumKind> = None;
    let mut declared: Vec<DeclaredFrame> = Vec::new();

    let mut current_label: Option<String> = None;
    let mut current_pairs: Vec<(String, String)> = Vec::new();

    for (line_no, line) in text.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some(rest) = line.strip_prefix("field,") {
            let parts: Vec<&str> = rest.splitn(2, ',').collect();
            let [kind_str, name] = parts[..] else {
                return Err(format!("line {}: expected \"field,kind,name\", got {line:?}", line_no + 1));
            };
            let kind = parse_kind(kind_str).map_err(|e| format!("line {}: {e}", line_no + 1))?;
            fields.push(FieldSpec { name: name.to_string(), kind });
            continue;
        }
        if let Some(rest) = line.strip_prefix("checksum,") {
            if checksum.is_some() {
                return Err(format!("line {}: checksum declared more than once", line_no + 1));
            }
            checksum = Some(parse_checksum_kind(rest).map_err(|e| format!("line {}: {e}", line_no + 1))?);
            continue;
        }
        if let Some(label) = line.strip_prefix("frame,") {
            if let Some(prev_label) = current_label.take() {
                declared.push(finish_frame(prev_label, &current_pairs, &fields)?);
                current_pairs.clear();
            }
            current_label = Some(label.to_string());
            continue;
        }
        if current_label.is_none() {
            return Err(format!("line {}: {line:?} outside any \"frame,<label>\" block", line_no + 1));
        }
        let Some((key, value)) = line.split_once('=') else {
            return Err(format!("line {}: expected \"key=value\", got {line:?}", line_no + 1));
        };
        current_pairs.push((key.trim().to_string(), value.trim().to_string()));
    }
    if let Some(label) = current_label.take() {
        declared.push(finish_frame(label, &current_pairs, &fields)?);
    }

    let checksum = checksum.ok_or_else(|| "missing \"checksum,<kind>\" line".to_string())?;
    if fields.is_empty() {
        return Err("no \"field,...\" lines declared".to_string());
    }
    Ok((FrameSpec { fields, checksum }, declared))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn u16_endianness_packs_in_the_declared_byte_order() {
        let mut le = Vec::new();
        pack_field(FieldKind::U16Le, &FieldValue::Number(0x1234), &mut le).unwrap();
        assert_eq!(le, vec![0x34, 0x12]);

        let mut be = Vec::new();
        pack_field(FieldKind::U16Be, &FieldValue::Number(0x1234), &mut be).unwrap();
        assert_eq!(be, vec![0x12, 0x34]);
    }

    #[test]
    fn u32_endianness_packs_in_the_declared_byte_order() {
        let mut le = Vec::new();
        pack_field(FieldKind::U32Le, &FieldValue::Number(0x0102_0304), &mut le).unwrap();
        assert_eq!(le, vec![0x04, 0x03, 0x02, 0x01]);

        let mut be = Vec::new();
        pack_field(FieldKind::U32Be, &FieldValue::Number(0x0102_0304), &mut be).unwrap();
        assert_eq!(be, vec![0x01, 0x02, 0x03, 0x04]);
    }

    fn boot_status_spec() -> FrameSpec {
        FrameSpec {
            fields: vec![
                FieldSpec { name: "version".to_string(), kind: FieldKind::U8 },
                FieldSpec { name: "length".to_string(), kind: FieldKind::U16Be },
                FieldSpec { name: "device_id".to_string(), kind: FieldKind::Bytes(4) },
            ],
            checksum: ChecksumKind::Crc32,
        }
    }

    #[test]
    fn build_frame_trailer_matches_the_real_crc32_of_the_payload() {
        let spec = boot_status_spec();
        let values =
            vec![FieldValue::Number(1), FieldValue::Number(9), FieldValue::Bytes(vec![0xde, 0xad, 0xbe, 0xef])];
        let frame = build_frame(&spec, &values).unwrap();
        assert_eq!(frame.len(), 7 + 4);
        let payload = &frame[..7];
        let trailer = &frame[7..];
        assert_eq!(trailer, crc32(payload).to_be_bytes());
    }

    #[test]
    fn a_built_frame_inspects_back_to_the_same_field_values_with_a_valid_checksum() {
        let spec = boot_status_spec();
        let values =
            vec![FieldValue::Number(1), FieldValue::Number(9), FieldValue::Bytes(vec![0xde, 0xad, 0xbe, 0xef])];
        let frame = build_frame(&spec, &values).unwrap();
        let inspected = inspect_frame(&spec, &frame).unwrap();
        assert!(inspected.checksum_ok);
        assert_eq!(inspected.fields, vec![
            ("version".to_string(), FieldValue::Number(1)),
            ("length".to_string(), FieldValue::Number(9)),
            ("device_id".to_string(), FieldValue::Bytes(vec![0xde, 0xad, 0xbe, 0xef])),
        ]);
    }

    #[test]
    fn a_corrupted_trailer_is_caught_not_silently_accepted() {
        let spec = boot_status_spec();
        let values =
            vec![FieldValue::Number(1), FieldValue::Number(9), FieldValue::Bytes(vec![0xde, 0xad, 0xbe, 0xef])];
        let mut frame = build_frame(&spec, &values).unwrap();
        *frame.last_mut().unwrap() ^= 0x01;
        let inspected = inspect_frame(&spec, &frame).unwrap();
        assert!(!inspected.checksum_ok);
        // fields still decode -- an inspector reports a bad checksum,
        // it doesn't refuse to show what the bytes actually contain.
        assert_eq!(inspected.fields[0], ("version".to_string(), FieldValue::Number(1)));
    }

    #[test]
    fn inspect_frame_rejects_the_wrong_total_length() {
        let spec = boot_status_spec();
        let err = inspect_frame(&spec, &[0u8; 3]).unwrap_err();
        assert!(err.contains("expected 11"), "error should name the expected length: {err}");
    }

    #[test]
    fn parse_frame_format_reads_a_real_declaration() {
        let text = "\
# boot-status header
field,u8,version
field,u16be,length
field,bytes4,device_id
checksum,crc32

frame,boot-status-ok
version=1
length=9
device_id=deadbeef
";
        let (spec, frames) = parse_frame_format(text).unwrap();
        assert_eq!(spec, boot_status_spec());
        assert_eq!(frames.len(), 1);
        assert_eq!(frames[0].label, "boot-status-ok");
        assert_eq!(
            frames[0].values,
            vec![FieldValue::Number(1), FieldValue::Number(9), FieldValue::Bytes(vec![0xde, 0xad, 0xbe, 0xef])]
        );
    }

    #[test]
    fn parse_frame_format_accepts_hex_numeric_values() {
        let text = "field,u16be,x\nchecksum,crc32\nframe,f\nx=0x00ff\n";
        let (_, frames) = parse_frame_format(text).unwrap();
        assert_eq!(frames[0].values, vec![FieldValue::Number(255)]);
    }

    #[test]
    fn parse_frame_format_rejects_a_missing_field_value() {
        let text = "field,u8,version\nfield,u16be,length\nchecksum,crc32\nframe,f\nversion=1\n";
        let err = parse_frame_format(text).unwrap_err();
        assert!(err.contains("length"), "error should name the missing field: {err}");
    }

    #[test]
    fn parse_frame_format_rejects_a_missing_checksum_line() {
        let text = "field,u8,version\nframe,f\nversion=1\n";
        let err = parse_frame_format(text).unwrap_err();
        assert!(err.contains("checksum"), "error should name what's missing: {err}");
    }
}
