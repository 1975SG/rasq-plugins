//! CRC/checksum tools (`docs/roadmap/Workbenches.md`'s Protocol/systems
//! analysis tooling category) -- the frame/packet-inspector half of that
//! same roadmap line is out of scope here, a separate, larger design
//! pass (what does "a frame" mean generically enough to build, per
//! ADR-011). This crate is the verification primitive a frame/packet
//! inspector would eventually consume, not the inspector itself.
//!
//! every algorithm here is a bit-by-bit implementation (no lookup
//! table) -- slower than a real production CRC library, deliberately:
//! auditable against the algorithm's own definition line by line, and
//! the data volumes this project's own example plugins ever process are
//! nowhere near where that would matter. Pure computation, no I/O, no
//! external dependency, same shape as `deviation-analyzer`/
//! `distribution-analysis`.
//!
//! every CRC variant is verified in this crate's own tests against its
//! published "check value" for the ASCII string `"123456789"` -- the
//! standard cross-implementation verification technique for CRC
//! algorithms (the CRC RevEng catalogue's own convention), not just
//! internal self-consistency.

/// CRC-32/ISO-HDLC -- the "CRC-32" almost everything means by that name
/// unqualified (zlib, gzip, PNG, Ethernet FCS). Reflected polynomial
/// `0xEDB88320`, init `0xFFFFFFFF`, input/output reflected, final XOR
/// `0xFFFFFFFF`.
pub fn crc32(data: &[u8]) -> u32 {
    let mut crc: u32 = 0xFFFF_FFFF;
    for &byte in data {
        crc ^= byte as u32;
        for _ in 0..8 {
            crc = if crc & 1 != 0 { (crc >> 1) ^ 0xEDB8_8320 } else { crc >> 1 };
        }
    }
    crc ^ 0xFFFF_FFFF
}

/// CRC-16/XMODEM -- polynomial `0x1021`, init `0x0000`, MSB-first, no
/// reflection, no final XOR. Common in embedded/serial protocols.
pub fn crc16_xmodem(data: &[u8]) -> u16 {
    let mut crc: u16 = 0x0000;
    for &byte in data {
        crc ^= (byte as u16) << 8;
        for _ in 0..8 {
            crc = if crc & 0x8000 != 0 { (crc << 1) ^ 0x1021 } else { crc << 1 };
        }
    }
    crc
}

/// CRC-16/MODBUS -- polynomial `0x8005` (reflected `0xA001`), init
/// `0xFFFF`, LSB-first (input/output reflected), no final XOR. The
/// industrial-protocol variant, a different check value from XMODEM
/// despite both being "CRC-16" -- the two aren't interchangeable.
pub fn crc16_modbus(data: &[u8]) -> u16 {
    let mut crc: u16 = 0xFFFF;
    for &byte in data {
        crc ^= byte as u16;
        for _ in 0..8 {
            crc = if crc & 1 != 0 { (crc >> 1) ^ 0xA001 } else { crc >> 1 };
        }
    }
    crc
}

/// RFC 1071 Internet checksum (IP/TCP/UDP headers) -- ones-complement
/// sum of 16-bit big-endian words, carries folded back in, then
/// ones-complemented. An odd-length input's last byte is treated as the
/// high byte of a final word padded with a zero low byte, per the RFC.
pub fn internet_checksum(data: &[u8]) -> u16 {
    let mut sum: u32 = 0;
    let mut chunks = data.chunks_exact(2);
    for chunk in &mut chunks {
        sum += u16::from_be_bytes([chunk[0], chunk[1]]) as u32;
    }
    if let [last] = chunks.remainder() {
        sum += (*last as u32) << 8;
    }
    while sum >> 16 != 0 {
        sum = (sum & 0xFFFF) + (sum >> 16);
    }
    !(sum as u16)
}

#[cfg(test)]
mod tests {
    use super::*;

    const CHECK_INPUT: &[u8] = b"123456789";

    #[test]
    fn crc32_matches_the_published_check_value() {
        assert_eq!(crc32(CHECK_INPUT), 0xCBF4_3926);
    }

    #[test]
    fn crc16_xmodem_matches_the_published_check_value() {
        assert_eq!(crc16_xmodem(CHECK_INPUT), 0x31C3);
    }

    #[test]
    fn crc16_modbus_matches_the_published_check_value() {
        assert_eq!(crc16_modbus(CHECK_INPUT), 0x4B37);
    }

    #[test]
    fn crc32_of_empty_input_is_zero() {
        // init/final-XOR pair cancels out exactly when no bytes are
        // processed -- a real property of this algorithm, not a
        // special-cased default.
        assert_eq!(crc32(&[]), 0);
    }

    #[test]
    fn flipping_one_bit_changes_the_crc() {
        let original = crc32(b"hello world");
        let corrupted = crc32(b"hemlo world");
        assert_ne!(original, corrupted);
    }

    #[test]
    fn xmodem_and_modbus_disagree_on_the_same_input() {
        // both are called "CRC-16" but use different polynomials/init
        // values/reflection -- verifying they're actually distinct
        // algorithms, not two names for the same computation.
        assert_ne!(crc16_xmodem(CHECK_INPUT), crc16_modbus(CHECK_INPUT));
    }

    #[test]
    fn internet_checksum_of_a_word_is_its_ones_complement() {
        assert_eq!(internet_checksum(&[0x00, 0x01]), !0x0001u16);
    }

    #[test]
    fn internet_checksum_verifies_a_packet_including_its_own_checksum_field() {
        // standard verification property real IP stacks actually
        // use (RFC 1071 section 4.1): recomputing the checksum over a
        // packet's words *including* its own correct checksum field
        // gives zero. (The raw pre-complement sum is all-ones,
        // 0xFFFF -- `internet_checksum` complements that, so the
        // function's own return value on a valid packet is 0x0000, not
        // 0xFFFF.)
        let payload = [0x45u8, 0x00, 0x00, 0x3c, 0x1c, 0x46, 0x40, 0x00, 0x40, 0x06];
        let checksum = internet_checksum(&payload);
        let mut with_checksum = payload.to_vec();
        with_checksum.extend_from_slice(&checksum.to_be_bytes());
        assert_eq!(internet_checksum(&with_checksum), 0x0000);
    }

    #[test]
    fn internet_checksum_handles_an_odd_length_input() {
        // exercises the "pad the trailing byte" path specifically --
        // easy to get backwards (high byte vs low byte) without a
        // dedicated test for it. Hand-computed not chained
        // through the "append checksum, re-verify" property above:
        // appending a 2-byte checksum to a 3-byte (odd-length) payload
        // produces a 5-byte (still odd) sequence, which doesn't word-
        // align the same way real checksum-bearing protocols do (their
        // header lengths are always chosen even specifically so the
        // checksum field itself never straddles the padding case).
        // 0x0001 (word) + 0x0200 (0x02 padded as a word's high byte) =
        // 0x0201; the checksum is that value's ones' complement.
        assert_eq!(internet_checksum(&[0x00, 0x01, 0x02]), !0x0201u16);
    }
}
