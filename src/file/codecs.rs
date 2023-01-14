use crate::file::VersionCodec;

pub struct Codec0_0_1 {}

impl Codec0_0_1 {
    pub const fn new() -> Self {
        Self {}
    }
}

impl VersionCodec for Codec0_0_1 {}
