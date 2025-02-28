use std::collections::HashMap;
use std::fmt;
use std::hash::{Hash, Hasher};

use crate::extn::core::regexp::{Config, Encoding};
use crate::extn::prelude::*;

pub mod lazy;
#[cfg(feature = "core-regexp-oniguruma")]
pub mod onig;
pub mod regex;

pub type NilableString = Option<Vec<u8>>;
pub type NameToCaptureLocations = Vec<(Vec<u8>, Vec<usize>)>;

#[derive(Debug, Clone, Hash, PartialEq, Eq, PartialOrd, Ord)]
pub enum Scan {
    Collected(Vec<Vec<Option<Vec<u8>>>>),
    Patterns(Vec<Vec<u8>>),
    Haystack,
}

impl TryConvertMut<Scan, Option<Value>> for Artichoke {
    type Error = Exception;

    fn try_convert_mut(&mut self, from: Scan) -> Result<Option<Value>, Self::Error> {
        match from {
            Scan::Collected(collected) => {
                let ary = self.try_convert_mut(collected)?;
                Ok(Some(ary))
            }
            Scan::Patterns(patterns) => {
                let ary = self.try_convert_mut(patterns)?;
                Ok(Some(ary))
            }
            Scan::Haystack => Ok(None),
        }
    }
}

pub trait RegexpType {
    fn box_clone(&self) -> Box<dyn RegexpType>;

    fn debug(&self) -> String;

    fn literal_config(&self) -> &Config;

    fn derived_config(&self) -> &Config;

    fn encoding(&self) -> &Encoding;

    fn inspect(&self) -> Vec<u8>;

    fn string(&self) -> &[u8];

    fn captures(&self, haystack: &[u8]) -> Result<Option<Vec<NilableString>>, Exception>;

    fn capture_indexes_for_name(&self, name: &[u8]) -> Result<Option<Vec<usize>>, Exception>;

    fn captures_len(&self, haystack: Option<&[u8]>) -> Result<usize, Exception>;

    fn capture0<'a>(&self, haystack: &'a [u8]) -> Result<Option<&'a [u8]>, Exception>;

    fn case_match(&self, interp: &mut Artichoke, haystack: &[u8]) -> Result<bool, Exception>;

    fn is_match(&self, haystack: &[u8], pos: Option<Int>) -> Result<bool, Exception>;

    fn match_(
        &self,
        interp: &mut Artichoke,
        haystack: &[u8],
        pos: Option<Int>,
        block: Option<Block>,
    ) -> Result<Value, Exception>;

    fn match_operator(
        &self,
        interp: &mut Artichoke,
        haystack: &[u8],
    ) -> Result<Option<usize>, Exception>;

    fn named_captures(&self) -> Result<NameToCaptureLocations, Exception>;

    fn named_captures_for_haystack(
        &self,
        haystack: &[u8],
    ) -> Result<Option<HashMap<Vec<u8>, NilableString>>, Exception>;

    fn names(&self) -> Vec<Vec<u8>>;

    fn pos(&self, haystack: &[u8], at: usize) -> Result<Option<(usize, usize)>, Exception>;

    fn scan(
        &self,
        interp: &mut Artichoke,
        haystack: &[u8],
        block: Option<Block>,
    ) -> Result<Scan, Exception>;
}

impl Clone for Box<dyn RegexpType> {
    #[inline]
    fn clone(&self) -> Self {
        self.box_clone()
    }
}

impl fmt::Debug for Box<dyn RegexpType> {
    #[inline]
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(&self.as_ref(), f)
    }
}

impl Hash for Box<dyn RegexpType> {
    #[inline]
    fn hash<H: Hasher>(&self, state: &mut H) {
        Hash::hash(&self.as_ref(), state)
    }
}

impl PartialEq for Box<dyn RegexpType> {
    #[inline]
    fn eq(&self, other: &Self) -> bool {
        PartialEq::eq(&self.as_ref(), &other.as_ref())
    }
}

impl Eq for Box<dyn RegexpType> {}

impl fmt::Debug for &dyn RegexpType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.debug())
    }
}

impl Hash for &dyn RegexpType {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.literal_config().hash(state);
    }
}

impl PartialEq for &dyn RegexpType {
    fn eq(&self, other: &Self) -> bool {
        self.derived_config().pattern == other.derived_config().pattern
            && self.encoding() == other.encoding()
    }
}

impl Eq for &dyn RegexpType {}

impl fmt::Debug for &mut dyn RegexpType {
    #[inline]
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(&&*self, f)
    }
}

impl Hash for &mut dyn RegexpType {
    #[inline]
    fn hash<H: Hasher>(&self, state: &mut H) {
        Hash::hash(&&*self, state);
    }
}

impl PartialEq for &mut dyn RegexpType {
    #[inline]
    fn eq(&self, other: &Self) -> bool {
        PartialEq::eq(&&*self, &&*other)
    }
}

impl Eq for &mut dyn RegexpType {}
