// Copyright 2016 Francis Gagné
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

#![cfg_attr(feature = "serde_derive", feature(proc_macro))]

#![allow(unknown_lints)]

#[macro_use]
extern crate clap;
extern crate hyper;
extern crate mime;
#[macro_use]
extern crate quick_error;
extern crate serde;
#[cfg(feature = "serde_derive")]
#[macro_use]
extern crate serde_derive;
extern crate serde_json;
extern crate serde_yaml;
extern crate syndication;
extern crate url;

#[cfg(feature = "serde_derive")]
include!("main.rs.in");

#[cfg(not(feature = "serde_derive"))]
include!(concat!(env!("OUT_DIR"), "/main.rs"));
