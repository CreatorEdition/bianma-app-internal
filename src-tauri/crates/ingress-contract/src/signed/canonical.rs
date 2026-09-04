//! 版本化、长度前缀且严格顺序的 canonical TLV codec。

use crate::IngressReject;

const FIELD_HEADER_BYTES: usize = 6;

#[cfg(test)]
pub(crate) struct Encoder {
    bytes: Vec<u8>,
}

#[cfg(test)]
impl Encoder {
    pub(crate) fn with_prefix(prefix: &[u8]) -> Self {
        Self {
            bytes: prefix.to_vec(),
        }
    }

    pub(crate) fn field(&mut self, tag: u16, value: &[u8]) {
        let length = u32::try_from(value.len()).expect("canonical field length already bounded");
        self.bytes.extend_from_slice(&tag.to_be_bytes());
        self.bytes.extend_from_slice(&length.to_be_bytes());
        self.bytes.extend_from_slice(value);
    }

    pub(crate) fn field_u8(&mut self, tag: u16, value: u8) {
        self.field(tag, &[value]);
    }

    pub(crate) fn field_u16(&mut self, tag: u16, value: u16) {
        self.field(tag, &value.to_be_bytes());
    }

    pub(crate) fn field_u64(&mut self, tag: u16, value: u64) {
        self.field(tag, &value.to_be_bytes());
    }

    pub(crate) fn into_bytes(self) -> Vec<u8> {
        self.bytes
    }
}

pub(crate) struct Decoder<'a> {
    input: &'a [u8],
    offset: usize,
    error: IngressReject,
}

impl<'a> Decoder<'a> {
    pub(crate) fn with_prefix(
        input: &'a [u8],
        prefix: &[u8],
        error: IngressReject,
    ) -> Result<Self, IngressReject> {
        if !input.starts_with(prefix) {
            return Err(error);
        }
        Ok(Self {
            input,
            offset: prefix.len(),
            error,
        })
    }

    pub(crate) fn field(
        &mut self,
        expected_tag: u16,
        max_length: usize,
    ) -> Result<&'a [u8], IngressReject> {
        let header_end = self
            .offset
            .checked_add(FIELD_HEADER_BYTES)
            .ok_or(self.error)?;
        let header = self.input.get(self.offset..header_end).ok_or(self.error)?;
        let tag = u16::from_be_bytes([header[0], header[1]]);
        if tag != expected_tag {
            return Err(self.error);
        }
        let length = u32::from_be_bytes([header[2], header[3], header[4], header[5]]) as usize;
        if length > max_length {
            return Err(self.error);
        }
        let value_end = header_end.checked_add(length).ok_or(self.error)?;
        let value = self.input.get(header_end..value_end).ok_or(self.error)?;
        self.offset = value_end;
        Ok(value)
    }

    pub(crate) fn field_u8(&mut self, tag: u16) -> Result<u8, IngressReject> {
        let value = self.field(tag, 1)?;
        value.first().copied().ok_or(self.error)
    }

    pub(crate) fn field_u16(&mut self, tag: u16) -> Result<u16, IngressReject> {
        let value = self.field(tag, 2)?;
        let bytes: [u8; 2] = value.try_into().map_err(|_| self.error)?;
        Ok(u16::from_be_bytes(bytes))
    }

    pub(crate) fn field_u64(&mut self, tag: u16) -> Result<u64, IngressReject> {
        let value = self.field(tag, 8)?;
        let bytes: [u8; 8] = value.try_into().map_err(|_| self.error)?;
        Ok(u64::from_be_bytes(bytes))
    }

    pub(crate) fn finish(self) -> Result<(), IngressReject> {
        if self.offset == self.input.len() {
            Ok(())
        } else {
            Err(self.error)
        }
    }
}

pub(crate) fn fixed<const N: usize>(
    value: &[u8],
    error: IngressReject,
) -> Result<[u8; N], IngressReject> {
    value.try_into().map_err(|_| error)
}

pub(crate) fn bool_from_byte(value: u8, error: IngressReject) -> Result<bool, IngressReject> {
    match value {
        0 => Ok(false),
        1 => Ok(true),
        _ => Err(error),
    }
}
