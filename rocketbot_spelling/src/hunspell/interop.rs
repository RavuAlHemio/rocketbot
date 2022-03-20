use std::os::raw::{c_char, c_int, c_void};
use std::ops::Deref;

use libloading::{Library, library_filename, Symbol};
use once_cell::sync::OnceCell;

use crate::hunspell::HunspellError;


static HUNSPELL_LIBRARY: OnceCell<Library> = OnceCell::new();


#[derive(Clone)]
pub(crate) struct HunspellApi {
    create_key: Symbol<'static, unsafe extern fn(*const c_char, *const c_char, *const c_char) -> *mut c_void>,
    destroy: Symbol<'static, unsafe extern fn(*mut c_void)>,
    add_dic: Symbol<'static, unsafe extern fn(*mut c_void, *const c_char) -> c_int>,
    spell: Symbol<'static, unsafe extern fn(*mut c_void, *const c_char) -> c_int>,
    suggest: Symbol<'static, unsafe extern fn(*mut c_void, *mut *mut *mut c_char, *const c_char) -> c_int>,
    free_list: Symbol<'static, unsafe extern fn(*mut c_void, *mut *mut *mut c_char, c_int)>,
}
impl HunspellApi {
    fn get_or_load() -> Result<&'static Library, HunspellError> {
        if let Some(lib) = HUNSPELL_LIBRARY.get() {
            return Ok(lib);
        }

        let library = unsafe {
            Library::new(library_filename("hunspell"))
                .map_err(|e| HunspellError::LoadingFailed(e))?
        };
        let _ = HUNSPELL_LIBRARY.set(library);
        Ok(HUNSPELL_LIBRARY.get().expect("library unset right after setting?!"))
    }

    pub fn new() -> Result<Self, HunspellError> {
        let library = Self::get_or_load()?;

        unsafe {
            let create_key = library.get(b"Hunspell_create_key\0")
                .map_err(|e| HunspellError::SymbolNotFound("Hunspell_create_key", e))?;
            let destroy = library.get(b"Hunspell_destroy\0")
                .map_err(|e| HunspellError::SymbolNotFound("Hunspell_destroy", e))?;
            let add_dic = library.get(b"Hunspell_add_dic\0")
                .map_err(|e| HunspellError::SymbolNotFound("Hunspell_add_dic", e))?;
            let spell = library.get(b"Hunspell_spell\0")
                .map_err(|e| HunspellError::SymbolNotFound("Hunspell_spell", e))?;
            let suggest = library.get(b"Hunspell_suggest\0")
                .map_err(|e| HunspellError::SymbolNotFound("Hunspell_suggest", e))?;
            let free_list = library.get(b"Hunspell_free_list\0")
                .map_err(|e| HunspellError::SymbolNotFound("Hunspell_free_list", e))?;

            Ok(Self {
                create_key,
                destroy,
                add_dic,
                spell,
                suggest,
                free_list,
            })
        }
    }

    pub unsafe fn create_key(&self, affpath: *const c_char, dpath: *const c_char, key: *const c_char) -> *mut c_void {
        self.create_key.deref()(affpath, dpath, key)
    }
    pub unsafe fn destroy(&self, handle: *mut c_void) {
        self.destroy.deref()(handle)
    }
    pub unsafe fn add_dic(&self, handle: *mut c_void, dpath: *const c_char) -> c_int {
        self.add_dic.deref()(handle, dpath)
    }
    pub unsafe fn spell(&self, handle: *mut c_void, word: *const c_char) -> c_int {
        self.spell.deref()(handle, word)
    }
    pub unsafe fn suggest(&self, handle: *mut c_void, slst: *mut *mut *mut c_char, word: *const c_char) -> c_int {
        self.suggest.deref()(handle, slst, word)
    }
    pub unsafe fn free_list(&self, handle: *mut c_void, slst: *mut *mut *mut c_char, n: c_int) {
        self.free_list.deref()(handle, slst, n)
    }
}
