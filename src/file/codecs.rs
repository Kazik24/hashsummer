use crate::file::{MainHeader, VersionCodec};
use crate::HashArray;
use std::io::Read;

pub struct Codec0_0_1 {}

impl Codec0_0_1 {
    pub const fn new() -> Self {
        Self {}
    }
}

impl VersionCodec for Codec0_0_1 {
    fn decode_header_fields(&self, array: HashArray<57>, header: &mut MainHeader) -> std::io::Result<()> {
        Ok(())
    }

    fn decode_additional_header(&self, read: &mut dyn Read, header: &mut MainHeader) -> std::io::Result<()> {
        Ok(())
    }
}
