#![deny(clippy::all)]
#![deny(clippy::pedantic)]
#![allow(clippy::restriction)]

use chrono::prelude::*;
use std::env;
use std::ffi::OsString;
use std::fmt;
use std::process::Command;
use std::str::{self, FromStr};
use target_lexicon::Triple;

#[derive(Default, Debug, Clone, Copy, Hash, PartialEq, Eq, PartialOrd, Ord)]
struct Date {
    pub year: i32,
    pub month: u32,
    pub day: u32,
}

impl From<DateTime<Utc>> for Date {
    fn from(date: DateTime<Utc>) -> Self {
        Self::new(date.year(), date.month(), date.day())
    }
}

impl fmt::Display for Date {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:04}-{:02}-{:02}", self.year, self.month, self.day)
    }
}

impl Date {
    pub fn new(year: i32, month: u32, day: u32) -> Self {
        Self { year, month, day }
    }

    pub fn year(&self) -> i32 {
        self.year
    }

    pub fn month(&self) -> u32 {
        self.month
    }

    pub fn day(&self) -> u32 {
        self.day
    }
}

pub fn build_release_metadata(target: &Triple) {
    let version = env::var("CARGO_PKG_VERSION").unwrap();
    let birth_date = birthdate();
    let build_date = Date::from(Utc::now());
    let release_date = build_date;
    let revision_count = revision_count();
    let platform = platform(target);
    let copyright = copyright(birth_date, build_date);
    let description = description(
        version.as_str(),
        release_date,
        revision_count,
        platform.as_str(),
    );

    emit("RUBY_RELEASE_DATE", release_date);
    emit("RUBY_RELEASE_YEAR", build_date.year());
    emit("RUBY_RELEASE_MONTH", build_date.month());
    emit("RUBY_RELEASE_DAY", build_date.day());
    emit("RUBY_REVISION", revision_count.unwrap_or(0));
    emit("RUBY_PLATFORM", platform);
    emit("RUBY_COPYRIGHT", copyright);
    emit("RUBY_DESCRIPTION", description);
    emit(
        "ARTICHOKE_COMPILER_VERSION",
        compiler_version().unwrap_or_else(String::new),
    );
}

fn emit<T>(env: &str, value: T)
where
    T: fmt::Display,
{
    println!("cargo:rustc-env={}={}", env, value);
}

fn birthdate() -> Date {
    // $ git show -s --format="%ct" db318759dad41686be679c87c349fcb5ff0a396c
    // 1554600621
    // $ git show -s --format="%ci" db318759dad41686be679c87c349fcb5ff0a396c
    // 2019-04-06 18:30:21 -0700
    // $ git rev-list --count db318759dad41686be679c87c349fcb5ff0a396c
    // 1
    let time = 1_554_600_621;
    Utc.timestamp(time, 0).into()
}

fn revision_count() -> Option<usize> {
    let cmd = OsString::from("git");
    let revision_count = Command::new(cmd)
        .arg("rev-list")
        .arg("--count")
        .arg("HEAD")
        .output()
        .ok()?;
    String::from_utf8(revision_count.stdout)
        .ok()?
        .trim()
        .parse()
        .ok()
}

fn platform(target: &Triple) -> String {
    target.to_string()
}

fn copyright(birth_date: Date, build_date: Date) -> String {
    if birth_date.year() == build_date.year() {
        format!(
            "Copyright (c) {} Ryan Lopopolo <rjl@hyperbo.la>",
            birth_date.year()
        )
    } else {
        format!(
            "Copyright (c) {}-{} Ryan Lopopolo <rjl@hyperbo.la>",
            birth_date.year(),
            build_date.year()
        )
    }
}

fn description(
    version: &str,
    release_date: Date,
    revision_count: Option<usize>,
    platform: &str,
) -> String {
    if let Some(revision_count) = revision_count {
        format!(
            "artichoke {} ({} revision {}) [{}]",
            version, release_date, revision_count, platform
        )
    } else {
        format!("artichoke {} ({}) [{}]", version, release_date, platform)
    }
}

fn compiler_version() -> Option<String> {
    let cmd = env::var_os("RUSTC").unwrap_or_else(|| OsString::from("rustc"));
    let compiler_version = Command::new(cmd).arg("-V").output().ok()?;
    String::from_utf8(compiler_version.stdout).ok()
}

fn main() {
    let target = env::var_os("TARGET").unwrap();
    let target = Triple::from_str(target.to_str().unwrap()).unwrap();
    build_release_metadata(&target)
}
