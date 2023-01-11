use cfg_if::cfg_if;
use std::borrow::Cow;
use std::char::decode_utf16;
use std::ffi::OsStr;

pub fn convert_to_meaningful_str(os: &OsStr) -> Cow<'_, str> {
    if let Some(s) = os.to_str() {
        return Cow::Borrowed(s);
    }

    cfg_if! {
        if #[cfg(windows)] {

            // let v = os.encode_wide().collect::<Vec<_>>();
            // let mut s = String::new();
            // for res in decode_utf16(v) {
            //     match res {
            //         Ok(c) => s.push(c),
            //         Err(err) => {
            //
            //         }
            //     }
            // }
        }
    }

    //when all else fails
    os.to_string_lossy()
}
