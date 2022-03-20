mod interop;


use std::ffi::{CStr, CString, NulError};
use std::fmt;
use std::os::raw::{c_char, c_int, c_void};
use std::path::Path;
use std::ptr::{null, null_mut};

use crate::hunspell::interop::HunspellApi;


#[derive(Debug)]
pub enum HunspellError {
    LoadingFailed(libloading::Error),
    SymbolNotFound(&'static str, libloading::Error),
    NonUtf8AffixPath,
    NonUtf8DictionaryPath,
    NulInAffixPath(NulError),
    NulInDictionaryPath(NulError),
    NulInKey(NulError),
    NulInWord(NulError),
    NonUtf8Suggestion,
}
impl fmt::Display for HunspellError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::LoadingFailed(e)
                => write!(f, "failed to load hunspell library: {}", e),
            Self::SymbolNotFound(sym, e)
                => write!(f, "failed to load {:?} from hunspell library: {}", sym, e),
            Self::NonUtf8AffixPath
                => write!(f, "the affix path is not valid UTF-8"),
            Self::NonUtf8DictionaryPath
                => write!(f, "the dictionary path is not valid UTF-8"),
            Self::NulInAffixPath(e)
                => write!(f, "the affix path contains a NUL character: {}", e),
            Self::NulInDictionaryPath(e)
                => write!(f, "the dictionary path contains a NUL character: {}", e),
            Self::NulInKey(e)
                => write!(f, "the key contains a NUL character: {}", e),
            Self::NulInWord(e)
                => write!(f, "the word contains a NUL character: {}", e),
            Self::NonUtf8Suggestion
                => write!(f, "a suggestion is ont valid UTF-8"),
        }
    }
}
impl std::error::Error for HunspellError {
}


struct SuggestionArrayOwner {
    api: HunspellApi,
    handle: *mut c_void,
    array: *mut *mut c_char,
    array_length: c_int,
}
impl Drop for SuggestionArrayOwner {
    fn drop(&mut self) {
        unsafe {
            if self.array != null_mut() {
                self.api.free_list(self.handle, &mut self.array, self.array_length);
                self.array = null_mut();
            }
        }
    }
}


pub struct HunspellDictionary {
    api: HunspellApi,
    handle: *mut c_void,
}
impl HunspellDictionary {
    pub fn new(affix_path: &Path, dictionary_path: &Path, key: Option<String>) -> Result<Self, HunspellError> {
        let api = HunspellApi::new()?;

        let affix_path_c: CString = CString::new(
            affix_path.to_str()
                .ok_or(HunspellError::NonUtf8AffixPath)?
        )
            .map_err(|e| HunspellError::NulInAffixPath(e))?;

        let dictionary_path_c: CString = CString::new(
            dictionary_path.to_str()
                .ok_or(HunspellError::NonUtf8DictionaryPath)?
        )
            .map_err(|e| HunspellError::NulInDictionaryPath(e))?;

        let key_c = if let Some(k) = &key {
            Some(
                CString::new(k.as_bytes())
                    .map_err(|e| HunspellError::NulInKey(e))?
            )
        } else {
            None
        };
        let key_ptr = if let Some(k) = &key_c {
            k.as_ptr()
        } else {
            null()
        };

        let handle = unsafe {
            api.create_key(affix_path_c.as_ptr(), dictionary_path_c.as_ptr(), key_ptr)
        };

        Ok(Self {
            api,
            handle,
        })
    }

    /// Returns whether adding the dictionary was successful.
    pub fn add_dictionary(&mut self, dictionary_path: &Path) -> Result<bool, HunspellError> {
        let dictionary_path_c: CString = CString::new(
            dictionary_path.to_str()
                .ok_or(HunspellError::NonUtf8DictionaryPath)?
        )
            .map_err(|e| HunspellError::NulInDictionaryPath(e))?;

        let result = unsafe {
            self.api.add_dic(self.handle, dictionary_path_c.as_ptr())
        };
        Ok(result == 0)
    }

    /// Returns whether the word has been spelled correctly.
    pub fn spell(&self, word: &str) -> Result<bool, HunspellError> {
        let word_c = CString::new(word)
            .map_err(|e| HunspellError::NulInWord(e))?;
        let is_correct = unsafe {
            self.api.spell(self.handle, word_c.as_ptr())
        };
        Ok(is_correct != 0)
    }

    pub fn suggest(&self, word: &str) -> Result<Vec<String>, HunspellError> {
        let word_c = CString::new(word)
            .map_err(|e| HunspellError::NulInWord(e))?;
        let mut suggestions: Vec<String>;

        unsafe {
            let mut suggestions_ptr: *mut *mut c_char = null_mut();

            let suggest_count = self.api.suggest(self.handle, &mut suggestions_ptr, word_c.as_ptr());

            let suggestions_owner = SuggestionArrayOwner {
                api: self.api.clone(),
                handle: self.handle,
                array: suggestions_ptr,
                array_length: suggest_count,
            };

            let suggest_count_isize: isize = suggest_count.try_into()
                .expect("suggestion count does not fit into isize");
            let suggest_count_usize: usize = suggest_count.try_into()
                .expect("suggestion count does not fit into usize");

            suggestions = Vec::with_capacity(suggest_count_usize);

            for i in 0..suggest_count_isize {
                let suggestion_c = CStr::from_ptr(*suggestions_owner.array.offset(i));
                let suggestion = suggestion_c.to_str()
                    .map_err(|_| HunspellError::NonUtf8Suggestion)?
                    .to_owned();
                suggestions.push(suggestion);
            }
        }

        Ok(suggestions)
    }
}
impl Drop for HunspellDictionary {
    fn drop(&mut self) {
        unsafe {
            self.api.destroy(self.handle);
        }
        self.handle = null_mut();
    }
}
