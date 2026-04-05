use std::collections::{BTreeMap, BTreeSet};
use std::io::{self, Cursor, Read};

#[derive(Debug, Clone)]
pub struct StreamInput<'a> {
    cursor: Cursor<&'a [u8]>,
}

impl<'a> StreamInput<'a> {
    pub fn new(bytes: &'a [u8]) -> Self {
        Self {
            cursor: Cursor::new(bytes),
        }
    }

    pub fn remaining(&self) -> usize {
        self.cursor.get_ref().len() - self.cursor.position() as usize
    }

    pub fn read_u8(&mut self) -> io::Result<u8> {
        let mut buf = [0u8; 1];
        self.cursor.read_exact(&mut buf)?;
        Ok(buf[0])
    }

    pub fn read_bool(&mut self) -> io::Result<bool> {
        match self.read_u8()? {
            0 => Ok(false),
            1 => Ok(true),
            value => Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("invalid boolean byte {value}"),
            )),
        }
    }

    pub fn read_i32(&mut self) -> io::Result<i32> {
        let mut buf = [0u8; 4];
        self.cursor.read_exact(&mut buf)?;
        Ok(i32::from_be_bytes(buf))
    }

    pub fn read_u32(&mut self) -> io::Result<u32> {
        let mut buf = [0u8; 4];
        self.cursor.read_exact(&mut buf)?;
        Ok(u32::from_be_bytes(buf))
    }

    pub fn read_i64(&mut self) -> io::Result<i64> {
        let mut buf = [0u8; 8];
        self.cursor.read_exact(&mut buf)?;
        Ok(i64::from_be_bytes(buf))
    }

    pub fn read_u64(&mut self) -> io::Result<u64> {
        let mut buf = [0u8; 8];
        self.cursor.read_exact(&mut buf)?;
        Ok(u64::from_be_bytes(buf))
    }

    pub fn read_vint(&mut self) -> io::Result<u32> {
        let mut shift = 0u32;
        let mut value = 0u32;

        loop {
            let byte = self.read_u8()?;
            value |= ((byte & 0x7F) as u32) << shift;
            if (byte & 0x80) == 0 {
                return Ok(value);
            }

            shift += 7;
            if shift > 28 {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "invalid vint encoding",
                ));
            }
        }
    }

    pub fn read_bytes(&mut self, len: usize) -> io::Result<Vec<u8>> {
        let mut buf = vec![0u8; len];
        self.cursor.read_exact(&mut buf)?;
        Ok(buf)
    }

    pub fn read_byte_array(&mut self) -> io::Result<Vec<u8>> {
        let len = self.read_vint()? as usize;
        self.read_bytes(len)
    }

    pub fn read_string(&mut self) -> io::Result<String> {
        let len = self.read_vint()? as usize;
        let bytes = self.read_bytes(len)?;
        String::from_utf8(bytes).map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))
    }

    pub fn read_string_array(&mut self) -> io::Result<Vec<String>> {
        let len = self.read_vint()? as usize;
        let mut values = Vec::with_capacity(len);
        for _ in 0..len {
            values.push(self.read_string()?);
        }
        Ok(values)
    }

    pub fn read_string_map(&mut self) -> io::Result<BTreeMap<String, String>> {
        let len = self.read_vint()? as usize;
        let mut values = BTreeMap::new();
        for _ in 0..len {
            values.insert(self.read_string()?, self.read_string()?);
        }
        Ok(values)
    }

    pub fn read_string_list_map(&mut self) -> io::Result<BTreeMap<String, Vec<String>>> {
        let len = self.read_vint()? as usize;
        let mut values = BTreeMap::new();
        for _ in 0..len {
            values.insert(self.read_string()?, self.read_string_array()?);
        }
        Ok(values)
    }

    pub fn read_string_set_map(&mut self) -> io::Result<BTreeMap<String, BTreeSet<String>>> {
        let len = self.read_vint()? as usize;
        let mut values = BTreeMap::new();
        for _ in 0..len {
            let key = self.read_string()?;
            let set = self
                .read_string_array()?
                .into_iter()
                .collect::<BTreeSet<String>>();
            values.insert(key, set);
        }
        Ok(values)
    }
}

#[derive(Debug, Clone, Default)]
pub struct StreamOutput {
    bytes: Vec<u8>,
}

impl StreamOutput {
    pub fn new() -> Self {
        Self { bytes: Vec::new() }
    }

    pub fn into_bytes(self) -> Vec<u8> {
        self.bytes
    }

    pub fn write_bytes(&mut self, bytes: &[u8]) {
        self.bytes.extend_from_slice(bytes);
    }

    pub fn write_u8(&mut self, value: u8) {
        self.bytes.push(value);
    }

    pub fn write_bool(&mut self, value: bool) {
        self.write_u8(if value { 1 } else { 0 });
    }

    pub fn write_i32(&mut self, value: i32) {
        self.write_bytes(&value.to_be_bytes());
    }

    pub fn write_u32(&mut self, value: u32) {
        self.write_bytes(&value.to_be_bytes());
    }

    pub fn write_i64(&mut self, value: i64) {
        self.write_bytes(&value.to_be_bytes());
    }

    pub fn write_u64(&mut self, value: u64) {
        self.write_bytes(&value.to_be_bytes());
    }

    pub fn write_vint(&mut self, mut value: u32) {
        while (value & !0x7F) != 0 {
            self.write_u8(((value & 0x7F) as u8) | 0x80);
            value >>= 7;
        }
        self.write_u8(value as u8);
    }

    pub fn write_byte_array(&mut self, value: &[u8]) {
        self.write_vint(value.len() as u32);
        self.write_bytes(value);
    }

    pub fn write_string(&mut self, value: &str) {
        self.write_vint(value.as_bytes().len() as u32);
        self.write_bytes(value.as_bytes());
    }

    pub fn write_string_array(&mut self, values: &[String]) {
        self.write_vint(values.len() as u32);
        for value in values {
            self.write_string(value);
        }
    }

    pub fn write_string_map(&mut self, values: &BTreeMap<String, String>) {
        self.write_vint(values.len() as u32);
        for (key, value) in values {
            self.write_string(key);
            self.write_string(value);
        }
    }

    pub fn write_string_list_map(&mut self, values: &BTreeMap<String, Vec<String>>) {
        self.write_vint(values.len() as u32);
        for (key, value) in values {
            self.write_string(key);
            self.write_string_array(value);
        }
    }

    pub fn write_string_set_map(&mut self, values: &BTreeMap<String, BTreeSet<String>>) {
        self.write_vint(values.len() as u32);
        for (key, value) in values {
            self.write_string(key);
            let items = value.iter().cloned().collect::<Vec<_>>();
            self.write_string_array(&items);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{StreamInput, StreamOutput};

    #[test]
    fn vint_round_trips() {
        let values = [0u32, 1, 127, 128, 255, 16_384, 3000099];

        let mut output = StreamOutput::new();
        for value in values {
            output.write_vint(value);
        }

        let bytes = output.into_bytes();
        let mut input = StreamInput::new(&bytes);

        for value in values {
            assert_eq!(input.read_vint().unwrap(), value);
        }
    }
}
