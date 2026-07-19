#[cfg(any(test, target_os = "windows", all(unix, not(target_os = "macos"))))]
use super::MonitorIdentificationError;

#[cfg(any(test, target_os = "windows", all(unix, not(target_os = "macos"))))]
pub(super) struct EdidEvidence;

#[cfg(any(test, target_os = "windows", all(unix, not(target_os = "macos"))))]
impl EdidEvidence {
    const BLOCK_SIZE: usize = 128;
    const DESCRIPTOR_END: usize = 126;
    const DESCRIPTOR_SIZE: usize = 18;
    const DESCRIPTOR_START: usize = 54;
    const EXTENSION_COUNT_INDEX: usize = 126;
    const HEADER: [u8; 8] = [0x00, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0x00];
    #[cfg(all(unix, not(target_os = "macos")))]
    pub(super) const PROPERTY_FORMAT: u8 = 8;
    const SERIAL_DESCRIPTOR_TAG: u8 = 0xff;
    const SERIAL_END: usize = 16;
    const SERIAL_START: usize = 12;
    const TEXT_SERIAL_START: usize = 5;

    pub(super) fn qualify(edid: Vec<u8>) -> Result<Vec<u8>, MonitorIdentificationError> {
        if edid.len() < Self::BLOCK_SIZE || edid[..Self::HEADER.len()] != Self::HEADER {
            return Err(MonitorIdentificationError::InvalidStableIdentity);
        }
        let block_count = usize::from(edid[Self::EXTENSION_COUNT_INDEX])
            .checked_add(1)
            .ok_or(MonitorIdentificationError::InvalidStableIdentity)?;
        let declared_length = block_count
            .checked_mul(Self::BLOCK_SIZE)
            .ok_or(MonitorIdentificationError::InvalidStableIdentity)?;
        if edid.len() != declared_length
            || !edid.chunks_exact(Self::BLOCK_SIZE).all(|block| {
                block
                    .iter()
                    .fold(0_u8, |checksum, byte| checksum.wrapping_add(*byte))
                    == 0
            })
        {
            return Err(MonitorIdentificationError::InvalidStableIdentity);
        }

        let serial: [u8; 4] = edid[Self::SERIAL_START..Self::SERIAL_END]
            .try_into()
            .map_err(|_| MonitorIdentificationError::InvalidStableIdentity)?;
        let numeric_serial = u32::from_le_bytes(serial);
        if !matches!(numeric_serial, 0 | u32::MAX) {
            return Ok(edid);
        }

        edid[Self::DESCRIPTOR_START..Self::DESCRIPTOR_END]
            .chunks_exact(Self::DESCRIPTOR_SIZE)
            .any(Self::has_qualified_text_serial)
            .then_some(edid)
            .ok_or(MonitorIdentificationError::InvalidStableIdentity)
    }

    fn has_qualified_text_serial(descriptor: &[u8]) -> bool {
        if descriptor[..3] != [0, 0, 0]
            || descriptor[3] != Self::SERIAL_DESCRIPTOR_TAG
            || descriptor[4] != 0
        {
            return false;
        }
        let serial = trim_serial_padding(&descriptor[Self::TEXT_SERIAL_START..]);
        if serial.is_empty()
            || !serial
                .iter()
                .all(|byte| byte.is_ascii_graphic() || *byte == b' ')
            || serial.iter().all(|byte| *byte == b'0')
            || serial.iter().all(|byte| matches!(byte, b'f' | b'F'))
        {
            return false;
        }
        let Ok(serial) = std::str::from_utf8(serial) else {
            return false;
        };
        let serial_key = normalized_placeholder_key(serial);
        if serial_key.is_empty()
            || serial_key.bytes().all(|byte| byte == b'0')
            || serial_key.bytes().all(|byte| byte == b'F')
        {
            return false;
        }
        !matches!(
            serial_key.as_str(),
            "DEFAULT"
                | "GENERIC"
                | "NA"
                | "NONE"
                | "NOSERIAL"
                | "NOTAVAILABLE"
                | "NOTSPECIFIED"
                | "SERIAL"
                | "SERIALNUMBER"
                | "UNKNOWN"
                | "UNSPECIFIED"
        )
    }
}

#[cfg(any(test, target_os = "windows", all(unix, not(target_os = "macos"))))]
fn normalized_placeholder_key(serial: &str) -> String {
    serial
        .bytes()
        .filter(u8::is_ascii_alphanumeric)
        .map(|byte| char::from(byte.to_ascii_uppercase()))
        .collect()
}

#[cfg(any(test, target_os = "windows", all(unix, not(target_os = "macos"))))]
fn trim_serial_padding(serial: &[u8]) -> &[u8] {
    let is_padding = |byte: &u8| matches!(byte, 0 | b' ' | b'\t' | b'\n' | b'\r');
    let Some(start) = serial.iter().position(|byte| !is_padding(byte)) else {
        return &[];
    };
    let Some(end) = serial.iter().rposition(|byte| !is_padding(byte)) else {
        return &[];
    };
    &serial[start..=end]
}

#[cfg(test)]
mod tests {
    use super::*;

    const CHECKSUM_INDEX: usize = EdidEvidence::BLOCK_SIZE - 1;
    const NUMERIC_SERIAL: u32 = 42;

    fn synthetic_edid(extension_count: u8) -> Vec<u8> {
        let block_count = usize::from(extension_count) + 1;
        let mut edid = vec![0; block_count * EdidEvidence::BLOCK_SIZE];
        edid[..EdidEvidence::HEADER.len()].copy_from_slice(&EdidEvidence::HEADER);
        edid[EdidEvidence::EXTENSION_COUNT_INDEX] = extension_count;
        for extension in 1..block_count {
            edid[extension * EdidEvidence::BLOCK_SIZE] = 2;
        }
        update_checksums(&mut edid);
        edid
    }

    fn numeric_serial_edid(extension_count: u8) -> Vec<u8> {
        let mut edid = synthetic_edid(extension_count);
        edid[EdidEvidence::SERIAL_START..EdidEvidence::SERIAL_END]
            .copy_from_slice(&NUMERIC_SERIAL.to_le_bytes());
        update_checksums(&mut edid);
        edid
    }

    fn text_serial_edid(serial: &[u8]) -> Vec<u8> {
        let mut edid = synthetic_edid(0);
        let descriptor = &mut edid[EdidEvidence::DESCRIPTOR_START
            ..EdidEvidence::DESCRIPTOR_START + EdidEvidence::DESCRIPTOR_SIZE];
        descriptor[3] = EdidEvidence::SERIAL_DESCRIPTOR_TAG;
        descriptor[EdidEvidence::TEXT_SERIAL_START..].fill(b' ');
        descriptor[EdidEvidence::TEXT_SERIAL_START..EdidEvidence::TEXT_SERIAL_START + serial.len()]
            .copy_from_slice(serial);
        update_checksums(&mut edid);
        edid
    }

    fn update_checksums(edid: &mut [u8]) {
        for block in edid.chunks_exact_mut(EdidEvidence::BLOCK_SIZE) {
            block[CHECKSUM_INDEX] = 0;
            let sum = block[..CHECKSUM_INDEX]
                .iter()
                .fold(0_u8, |checksum, byte| checksum.wrapping_add(*byte));
            block[CHECKSUM_INDEX] = 0_u8.wrapping_sub(sum);
        }
    }

    #[test]
    fn edid_with_numeric_serial_preserves_complete_payload() {
        let edid = numeric_serial_edid(1);

        assert_eq!(EdidEvidence::qualify(edid.clone()), Ok(edid));
    }

    #[test]
    fn edid_with_text_serial_is_qualified() {
        let edid = text_serial_edid(b"SN-42-A");

        assert_eq!(EdidEvidence::qualify(edid.clone()), Ok(edid));
    }

    #[test]
    fn malformed_truncated_and_checksum_invalid_edids_are_rejected() {
        let mut malformed = numeric_serial_edid(0);
        malformed[0] = 1;
        update_checksums(&mut malformed);

        let mut truncated = numeric_serial_edid(1);
        truncated.pop();

        let mut invalid_checksum = numeric_serial_edid(1);
        invalid_checksum[EdidEvidence::BLOCK_SIZE] = 3;

        let mut incorrect_declared_length = numeric_serial_edid(0);
        incorrect_declared_length[EdidEvidence::EXTENSION_COUNT_INDEX] = 1;
        update_checksums(&mut incorrect_declared_length);

        assert!(EdidEvidence::qualify(malformed).is_err());
        assert!(EdidEvidence::qualify(truncated).is_err());
        assert!(EdidEvidence::qualify(invalid_checksum).is_err());
        assert!(EdidEvidence::qualify(incorrect_declared_length).is_err());
    }

    #[test]
    fn missing_and_placeholder_serials_are_rejected() {
        let mut all_ones = synthetic_edid(0);
        all_ones[EdidEvidence::SERIAL_START..EdidEvidence::SERIAL_END]
            .copy_from_slice(&u32::MAX.to_le_bytes());
        update_checksums(&mut all_ones);

        for placeholder in [
            b"".as_slice(),
            b"\0\0\0\0",
            b"000000",
            b"FFFFFFFF",
            b"UNKNOWN",
            b"default",
        ] {
            assert!(EdidEvidence::qualify(text_serial_edid(placeholder)).is_err());
        }
        assert!(EdidEvidence::qualify(synthetic_edid(0)).is_err());
        assert!(EdidEvidence::qualify(all_ones).is_err());
    }

    #[test]
    fn named_and_formatted_placeholder_serials_are_rejected() {
        for placeholder in [
            b"NO SERIAL".as_slice(),
            b"NOT AVAILABLE",
            b"not-available",
            b"000 000",
            b"00-00-00",
            b"FF FF FF",
            b"FF:FF:FF",
        ] {
            assert!(EdidEvidence::qualify(text_serial_edid(placeholder)).is_err());
        }
    }

    #[test]
    fn printable_punctuation_and_alphanumeric_serials_are_qualified() {
        for serial in [
            b"SN-42/A".as_slice(),
            b"A.B_C+42",
            b"SERIAL-42",
            b"000-001",
            b"FF-F1",
        ] {
            let edid = text_serial_edid(serial);

            assert_eq!(EdidEvidence::qualify(edid.clone()), Ok(edid));
        }
    }
}
