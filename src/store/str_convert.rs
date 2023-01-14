use cfg_if::cfg_if;
use std::borrow::Cow;
use std::char::decode_utf16;
use std::ffi::OsStr;
use std::marker::PhantomData;
use std::num::NonZeroUsize;

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

///Relative reference, where we can express self-referent struct with an offset, or global reference
pub struct RelRef<'a, T> {
    r: NonZeroUsize,
    relative: bool, //todo use alignment bits to signal relativity
    _phantom: PhantomData<&'a T>,
}

impl<'a, T> RelRef<'a, T> {
    pub fn global(r: &'a T) -> Self {
        Self {
            r: unsafe { NonZeroUsize::new_unchecked((r as *const T) as usize) },
            relative: false,
            _phantom: PhantomData,
        }
    }
    pub fn relative<P>(parent: &'a P, r: &'a T) -> Self {
        let p = (parent as *const P) as isize;
        let r = (r as *const T) as isize;
        let offset = ((r.wrapping_sub(p) as usize) << 1) | 1;
        Self {
            r: unsafe { NonZeroUsize::new_unchecked(offset) },
            relative: true,
            _phantom: PhantomData,
        }
    }

    pub fn get_global(&self) -> Option<&'a T> {
        if self.relative {
            return None;
        }
        let ptr = self.r.get() as *const T;
        unsafe { Some(&*ptr) }
    }

    pub unsafe fn get<P>(&self, parent: &P) -> &'a T {
        if let Some(v) = self.get_global() {
            return v;
        }
        let p = (parent as *const P) as isize;
        let offset = (self.r.get() as isize).wrapping_shr(1);
        let ptr = offset.wrapping_add(p) as *const T;
        &*ptr
    }
}
