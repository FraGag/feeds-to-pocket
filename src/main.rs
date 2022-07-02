// Copyright 2016 Francis Gagné
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

#![allow(unknown_lints)]

mod pocket;

use std::error::Error;
use std::fmt::{self, Display};
use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Write};
use std::path::Path;
use std::process;
use std::str::FromStr;

use clap::{crate_version, App, Arg, ArgMatches, SubCommand};
use quick_error::quick_error;
use reqwest::{Client, StatusCode};
use reqwest::header::{self, HeaderValue};
use serde::{Deserialize, Serialize};
use url::Url;

use crate::pocket::Pocket;

fn main() {
    let matches = App::new("Feeds to Pocket")
        .author("Francis Gagné <fragag1@gmail.com>")
        .about("Sends items from your RSS and Atom feeds to your Pocket list.")
        .version(crate_version!())
        .arg(Arg::with_name(args::CONFIG)
            .help("A YAML file containing your feeds configuration.")
            .required(true)
            .takes_value(true)
            .index(1))
        .subcommand(SubCommand::with_name(subcommands::init::NAME)
            .about("Creates an empty configuration file (if it doesn't already exist)."))
        .subcommand(SubCommand::with_name(subcommands::set_consumer_key::NAME)
            .about("Sets the consumer key in the configuration file.")
            .arg(Arg::with_name(subcommands::set_consumer_key::args::KEY)
                .help("A consumer key obtained from Pocket's website. \
                       You must create your own application \
                       at https://getpocket.com/developer/apps/new \
                       to obtain a consumer key; \
                       I don't want you kicking me out of my own application! :) \
                       Make sure your application has at least the \"Add\" permission.")
                .required(true)))
        .subcommand(SubCommand::with_name(subcommands::login::NAME)
            .about("Obtains and saves an access token from Pocket. \
                    This will print a URL on the standard output, \
                    which you must open in a web browser \
                    in order to grant your application access to your Pocket account. \
                    Once authorization has been obtained, \
                    an access token is saved in the configuration file, \
                    which will be used to queue up entries in your Pocket list."))
        .subcommand(SubCommand::with_name(subcommands::add::NAME)
            .about("Adds a feed to your feeds configuration or updates an existing feed in your feeds configuration.")
            .arg(Arg::with_name(subcommands::add::args::UNREAD)
                .long("--unread")
                .help("Consider all the entries in the feed to be unread. \
                       All entries will be sent to Pocket immediately. \
                       By default, all the entries present when the feed is added \
                       are considered read and are not sent to Pocket."))
            .arg(Arg::with_name(subcommands::add::args::TAGS)
                .long("--tags")
                .help("A comma-separated list of tags to attach to the URLs sent to Pocket.")
                .takes_value(true))
            .arg(Arg::with_name(subcommands::add::args::FEED_URL)
                .help("The URL of the feed to add.")
                .required(true)))
        .subcommand(SubCommand::with_name(subcommands::remove::NAME)
            .about("Removes a feed from your feeds configuration.")
            .arg(Arg::with_name(subcommands::remove::args::FEED_URL)
                .help("The URL of the feed to remove.")
                .required(true)))
        .get_matches();

    run(&matches).unwrap_or_else(|e| {
        let _ = writeln!(io::stderr(), "{}", e);
        process::exit(1);
    })
}

// Constants for command-line arguments and subcommands

mod args {
    pub const CONFIG: &'static str = "config";
}

mod subcommands {
    pub mod init {
        pub const NAME: &'static str = "init";
    }

    pub mod set_consumer_key {
        pub const NAME: &'static str = "set-consumer-key";

        pub mod args {
            pub const KEY: &'static str = "key";
        }
    }

    pub mod login {
        pub const NAME: &'static str = "login";
    }

    pub mod add {
        pub const NAME: &'static str = "add";

        pub mod args {
            pub const UNREAD: &'static str = "unread";
            pub const TAGS: &'static str = "tags";
            pub const FEED_URL: &'static str = "feed url";
        }
    }

    pub mod remove {
        pub const NAME: &'static str = "remove";

        pub mod args {
            pub const FEED_URL: &'static str = "feed url";
        }
    }
}

fn run(args: &ArgMatches) -> Result<(), ErrorWithContext> {
    if args.subcommand_name() == Some(subcommands::init::NAME) {
        init(args)
    } else {
        let mut config = load_config(args)?;

        // Dispatch based on the subcommand
        match args.subcommand() {
            ("", _) => sync(&mut config),
            (subcommands::set_consumer_key::NAME, Some(args)) => Ok(set_consumer_key(&mut config, &args)),
            (subcommands::login::NAME, _) => login(&mut config),
            (subcommands::add::NAME, Some(args)) => add(&mut config, &args),
            (subcommands::remove::NAME, Some(args)) => remove(&mut config, &args),
            (_, _) => unreachable!(),
        }?;

        save_config(&config, args)
    }
}

macro_rules! try_with_context {
    ($expr:expr, $context:expr) => (match $expr {
        ::std::result::Result::Ok(val) => val,
        ::std::result::Result::Err(err) => {
            return ::std::result::Result::Err($crate::ErrorWithContext::new(::std::convert::From::from(err), $context))
        }
    })
}

fn load_config(args: &ArgMatches) -> Result<Configuration, ErrorWithContext> {
    let config_file_name = args.value_of_os(args::CONFIG).unwrap();
    let config_file = try_with_context!(File::open(config_file_name),
        format!("failed to open file {}", config_file_name.to_string_lossy()));
    let config = try_with_context!(serde_yaml::from_reader(config_file),
        format!("failed to load configuration from {}", config_file_name.to_string_lossy()));
    Ok(config)
}

fn save_config(config: &Configuration, args: &ArgMatches) -> Result<(), ErrorWithContext> {
    let config_file_name = &args.value_of_os(args::CONFIG).unwrap();

    // Append ".new" to the config file name.
    // We'll write the updated configuration in this file,
    // then rename the original and the new files
    // to avoid corrupting the configuration.
    let new_config_file_name = &{
        let mut file_name = config_file_name.to_os_string();
        file_name.push(".new");
        file_name
    };

    // Append ".old" to the config file name.
    // We'll rename the original configuration file to this.
    let old_config_file_name = &{
        let mut file_name = config_file_name.to_os_string();
        file_name.push(".old");
        file_name
    };

    // Copy the configuration file, to preserve permissions.
    try_with_context!(fs::copy(config_file_name, new_config_file_name),
        format!("failed to copy {} to {}", config_file_name.to_string_lossy(), new_config_file_name.to_string_lossy()));

    // Write the updated configuration to the new configuration file.
    {
        let mut config_file = try_with_context!(File::create(new_config_file_name),
            format!("failed to create file {}", new_config_file_name.to_string_lossy()));
        try_with_context!(serde_yaml::to_writer(&mut config_file, config),
            format!("failed to save configuration to {}", new_config_file_name.to_string_lossy()));
    }

    fn rename<P: AsRef<Path> + Copy, Q: AsRef<Path> + Copy>(from: P, to: Q) -> Result<(), ErrorWithContext> {
        Ok(try_with_context!(fs::rename(from, to),
            format!("failed to rename {} to {}", from.as_ref().to_string_lossy(), to.as_ref().to_string_lossy())))
    }

    // Rename the original configuration file.
    rename(config_file_name, old_config_file_name)?;

    // Rename the new configuration file.
    let rename_new_result = rename(new_config_file_name, config_file_name);
    if rename_new_result.is_err() {
        // Rename the original configuration file back to its original name.
        let rollback_rename_old_result = rename(old_config_file_name, config_file_name);
        match rollback_rename_old_result {
            Ok(_) => return rename_new_result,
            Err(e) => try_with_context!(Err(Errors::new(vec![Box::new(rename_new_result.unwrap_err()), Box::new(e)])),
                "failed to save configuration"),
        }
    }

    // Delete the renamed original configuration file.
    try_with_context!(fs::remove_file(old_config_file_name),
        format!("failed to remove file {}", old_config_file_name.to_string_lossy()));

    Ok(())
}

fn init(args: &ArgMatches) -> Result<(), ErrorWithContext> {
    let config_file_name = args.value_of_os(args::CONFIG).unwrap();

    // Only write a configuration file if it doesn't exist yet.
    let mut config_file = try_with_context!(
        OpenOptions::new().write(true).create_new(true).open(config_file_name),
        format!("failed to create file {}", config_file_name.to_string_lossy()));

    let config = Configuration::default();
    try_with_context!(serde_yaml::to_writer(&mut config_file, &config),
        format!("failed to save configuration to {}", config_file_name.to_string_lossy()));

    Ok(())
}

fn set_consumer_key(config: &mut Configuration, args: &ArgMatches) {
    config.consumer_key = args.value_of(subcommands::set_consumer_key::args::KEY).map(String::from);
}

fn login(config: &mut Configuration) -> Result<(), ErrorWithContext> {
    let client = Client::new();
    let mut pocket = try_with_context!(get_pocket(config, client), "unable to perform authorization");

    if config.access_token.is_some() {
        println!("note: There's already an access token in the configuration file. \
            Proceeding will overwrite this access token.");
    }

    let auth_url = try_with_context!(pocket.get_auth_url(), "unable to get authorization URL for Pocket");
    println!("Go to the following webpage to login: {}", auth_url);
    println!("Then, press Enter to continue.");
    loop {
        // Let the user authorize access to the application before proceeding.
        let mut _input = String::new();
        try_with_context!(std::io::stdin().read_line(&mut _input), "unable to read from standard input");

        match pocket.authorize() {
            Ok(_) => {
                config.access_token = Some(String::from(pocket.access_token().unwrap()));
                return Ok(());
            }
            Err(e) => {
                println!("Authorization failed: {}\n\
                    Make sure you authorized your application at the webpage linked above.\n\
                    Press Enter to try again, or press Ctrl+C to exit.", e);
            }
        }
    }
}

fn sync(config: &mut Configuration) -> Result<(), ErrorWithContext> {
    let client = Client::new();
    let mut pocket = try_with_context!(get_authenticated_pocket(config, client.clone()), "unable to sync");

    for feed in &mut config.feeds {
        process_feed(feed, Some(&mut pocket), &client).unwrap_or_else(|e| {
            let _ = writeln!(io::stderr(), "{}", e);
        });
    }

    Ok(())
}

fn add(config: &mut Configuration, args: &ArgMatches) -> Result<(), ErrorWithContext> {
    fn apply_tags(feed: &mut FeedConfiguration, args: &ArgMatches) {
        if let Some(tags) = args.value_of(subcommands::add::args::TAGS) {
            feed.tags = tags.to_owned();
        }
    }

    let client = Client::new();

    let feed_url = args.value_of(subcommands::add::args::FEED_URL).unwrap();
    if let Some(feed) = config.feeds.iter_mut().find(|feed| feed.url == feed_url) {
        apply_tags(feed, args);
        return Ok(());
    }

    let send_to_pocket = args.is_present(subcommands::add::args::UNREAD);
    let mut pocket = if send_to_pocket {
        Some(try_with_context!(get_authenticated_pocket(config, client.clone()), "unable to add feed"))
    } else {
        None
    };

    let mut feed = FeedConfiguration {
        url: String::from(feed_url),
        tags: String::new(),
        processed_entries: vec![],
        last_modified: None,
        last_e_tag: None,
    };
    apply_tags(&mut feed, args);
    config.feeds.push(feed);

    let feed = config.feeds.last_mut().unwrap();

    process_feed(feed, pocket.as_mut(), &client)
}

fn remove(config: &mut Configuration, args: &ArgMatches) -> Result<(), ErrorWithContext> {
    let feed_url = args.value_of(subcommands::add::args::FEED_URL).unwrap();
    let len_before = config.feeds.len();
    config.feeds.retain(|feed| feed.url != feed_url);
    let len_after = config.feeds.len();
    if len_before == len_after {
        try_with_context!(Err(FeedNotFound::FeedNotFound(feed_url.into())), "failed to remove feed");
    }

    Ok(())
}

fn get_pocket(config: &Configuration, client: Client) -> Result<Pocket, PocketSetupError> {
    match config.consumer_key {
        Some(ref consumer_key) => Ok(Pocket::new(consumer_key, config.access_token.as_ref().map(|x| x.as_ref()), client)),
        None => Err(PocketSetupError::MissingConsumerKey),
    }
}

fn get_authenticated_pocket(config: &Configuration, client: Client) -> Result<Pocket, PocketSetupError> {
    get_pocket(config, client).and_then(|pocket| {
        match config.access_token {
            Some(_) => Ok(pocket),
            None => Err(PocketSetupError::MissingAccessToken),
        }
    })
}

fn process_feed(feed: &mut FeedConfiguration, mut pocket: Option<&mut Pocket>, client: &Client) -> Result<(), ErrorWithContext> {
    println!("downloading {}", feed.url);
    let feed_response = try_with_context!(fetch(feed, client),
        format!("failed to download feed at {url}", url=feed.url));

    // Do nothing if we received a 304 Not Modified response.
    if let FeedResponse::Success { body, last_modified, e_tag } = feed_response {
        let parsed_feed = try_with_context!(body.parse::<Feed>(),
            format!("failed to parse feed at {url} as either RSS or Atom", url=feed.url));

        let (mut rss_entries, mut atom_entries);
        let entries: &mut dyn Iterator<Item=&str> = match parsed_feed {
            Feed::RSS(ref rss) => {
                rss_entries = rss.items().iter().rev().flat_map(|item| item.link());
                &mut rss_entries
            }
            Feed::Atom(ref atom) => {
                atom_entries = atom.entries().into_iter().rev().flat_map(|entry| entry.links()).filter_map(|link| {
                    match link.rel() {
                        // Only push links with an "alternate" relation type.
                        "alternate" | "http://www.iana.org/assignments/relation/alternate" => Some(link.href()),
                        _ => None,
                    }
                });
                &mut atom_entries
            }
        };

        let mut all_processed_successfully = true;
        for entry_url in entries {
            // The rss and atom_syndication libraries
            // don't trim the values extracted from the XML files.
            let entry_url = entry_url.trim();

            // Ignore entries we've processed previously.
            if !feed.processed_entries.iter().rev().any(|x| x == &entry_url) {
                let is_processed =
                    if let Some(ref mut pocket) = pocket {
                        match Url::parse(&entry_url) {
                            Ok(parsed_entry_url) => {
                                // Push the entry to Pocket.
                                // Only consider the entry processed if the push succeeded.
                                // That means that if it failed, we'll try again next time.
                                println!("pushing {} to Pocket", entry_url);
                                let tags = if feed.tags.is_empty() { None } else { Some(&*feed.tags) };
                                let push_result = pocket.add(&parsed_entry_url, None, tags, None);
                                match push_result {
                                    Ok(_) => true,
                                    Err(error) => {
                                        println!("error while adding URL {url} to Pocket:\n  {error}",
                                            url=entry_url, error=Indented(&error));
                                        false
                                    }
                                }
                            }
                            Err(e) => {
                                println!("'{}' is not a valid URL ({}). ignoring.", entry_url, e);

                                // Mark the entry as processed,
                                // to avoid noise in subsequent runs.
                                true
                            }
                        }
                    } else {
                        // If `pocket` is None,
                        // then we just want to mark the current feed entries as processed,
                        // on the assumption that the user has read them already.
                        true
                    };

                if is_processed {
                    // Remember that we've processed this entry
                    // so we don't try to send it to Pocket next time.
                    feed.processed_entries.push(entry_url.into());
                } else {
                    all_processed_successfully = false;
                }
            }
        }

        // Don't update the last modified and last ETag
        // if any push to Pocket failed
        // so we can try again next time.
        if all_processed_successfully {
            feed.last_modified = last_modified.and_then(|v| v.to_str().ok().map(|s| s.into()));
            feed.last_e_tag = e_tag.and_then(|v| v.to_str().ok().map(|s| s.into()));
        }
    }

    Ok(())
}

fn fetch(feed: &FeedConfiguration, client: &Client) -> Result<FeedResponse, ErrorWithContext> {
    let mut request = client.get(&feed.url);
    request = request.header(header::USER_AGENT, HeaderValue::from_static(concat!("feeds-to-pocket/", env!("CARGO_PKG_VERSION"))));

    // Add an If-Modified-Since header if we have a Last-Modified date.
    if let Some(ref last_modified) = feed.last_modified {
        request = request.header(header::IF_MODIFIED_SINCE, HeaderValue::from_str(last_modified).expect("Failed to convert last_modified to HeaderValue"));
    }

    // Add an If-None-Match header if we have an ETag.
    if let Some(ref e_tag) = feed.last_e_tag {
        request = request.header(header::IF_NONE_MATCH, HeaderValue::from_str(e_tag).expect("Failed to convert last_e_tag to HeaderValue"));
    }

    let mut response = try_with_context!(request.send(),
        "failed to send request");
    if response.status() == StatusCode::NOT_MODIFIED {
        Ok(FeedResponse::NotModified)
    } else {
        if !response.status().is_success() {
            try_with_context!(Err(UnacceptableHttpStatus::UnacceptableHttpStatus(response.status())),
                format!("the HTTP request to <{}> didn't return a success status", feed.url));
        }

        let last_modified = response.headers().get(header::LAST_MODIFIED).cloned();
        let e_tag = response.headers().get(header::ETAG).cloned();

        let mut body = String::new();
        try_with_context!(response.read_to_string(&mut body),
            "failed to read response");

        Ok(FeedResponse::Success {
            body: body,
            last_modified: last_modified,
            e_tag: e_tag,
        })
    }
}

#[derive(Default, Deserialize, Serialize)]
struct Configuration {
    #[serde(skip_serializing_if="Option::is_none")]
    consumer_key: Option<String>,
    #[serde(skip_serializing_if="Option::is_none")]
    access_token: Option<String>,
    #[serde(skip_serializing_if="Vec::is_empty")]
    #[serde(default)]
    feeds: Vec<FeedConfiguration>,
}

#[derive(Deserialize, Serialize)]
struct FeedConfiguration {
    url: String,
    #[serde(skip_serializing_if="str::is_empty")]
    #[serde(default)]
    tags: String,
    #[serde(skip_serializing_if="Vec::is_empty")]
    #[serde(default)]
    processed_entries: Vec<String>,
    #[serde(skip_serializing_if="Option::is_none")]
    last_modified: Option<String>,
    #[serde(skip_serializing_if="Option::is_none")]
    last_e_tag: Option<String>,
}

enum FeedResponse {
    Success {
        body: String,
        last_modified: Option<HeaderValue>,
        e_tag: Option<HeaderValue>,
    },
    NotModified,
}

enum Feed {
    Atom(atom_syndication::Feed),
    RSS(rss::Channel),
}

impl FromStr for Feed {
    type Err = FeedError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.parse::<atom_syndication::Feed>() {
            Ok(feed) => Ok(Feed::Atom(feed)),
            Err(atom_error) => match s.parse::<rss::Channel>() {
                Ok(channel) => Ok(Feed::RSS(channel)),
                Err(rss_error) => Err(FeedError { atom_error, rss_error })
            }
        }
    }
}

#[derive(Debug)]
struct FeedError {
    atom_error: atom_syndication::Error,
    rss_error: rss::Error,
}

impl Display for FeedError {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        write!(fmt, "could not parse input as either Atom or RSS:\n  parsing as Atom failed with:\n    {}\n  parsing as RSS failed with:\n    {}",
            Indented(Indented(&self.atom_error)), Indented(Indented(&self.rss_error)))
    }
}

impl Error for FeedError {
    fn description(&self) -> &str {
        "could not parse input as either Atom or RSS"
    }
}

#[derive(Debug)]
struct ErrorWithContext {
    error: Box<dyn Error>,
    context: String
}

impl ErrorWithContext {
    fn new<S: Into<String>>(error: Box<dyn Error>, context: S) -> ErrorWithContext {
        ErrorWithContext {
            error: error,
            context: context.into(),
        }
    }
}

impl Display for ErrorWithContext {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        write!(fmt, "{}:\n  {}", self.context, Indented(&self.error))
    }
}

impl Error for ErrorWithContext {
    fn description(&self) -> &str {
        &self.context
    }

    fn cause(&self) -> Option<&dyn Error> {
        Some(&*self.error)
    }
}

quick_error! {
    #[allow(enum_variant_names)]
    #[derive(Debug)]
    enum PocketSetupError {
        MissingConsumerKey {
            description("The consumer key is not set in the configuration file. Run `feeds-to-pocket help set-consumer-key` for help and instructions.")
        }
        MissingAccessToken {
            description("The access token is not set in the configuration file. Run `feeds-to-pocket help login` for help and instructions.")
        }
    }
}

quick_error! {
    #[derive(Debug)]
    enum UnacceptableHttpStatus {
        UnacceptableHttpStatus(status: StatusCode) {
            display("{}", status)
        }
    }
}

quick_error! {
    #[derive(Debug)]
    enum Errors {
        Errors(errors: Vec<Box<dyn Error>>) {
            description("Multiple errors occurred.")
            display("{}", errors.iter().map(|error| format!("- {}", Indented(error))).collect::<Vec<_>>().join("\n"))
        }
    }
}

quick_error! {
    #[derive(Debug)]
    enum FeedNotFound {
        FeedNotFound(url: String) {
            display("No feed with URL {} was found.", url)
        }
    }
}

impl Errors {
    fn new(errors: Vec<Box<dyn Error>>) -> Errors {
        Errors::Errors(errors)
    }
}

/// Wraps a type implementing Display
/// and adds two spaces after each line feed in its display output.
struct Indented<D: Display>(D);

impl<D: Display> Display for Indented<D> {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> Result<(), fmt::Error> {
        use std::fmt::Write;
        write!(IndentedWrite(fmt), "{}", self.0)
    }
}

/// Intercepts writes to a `std::fmt::Formatter`
/// and adds two spaces after each line feed written to it.
struct IndentedWrite<'a: 'f, 'f>(&'f mut fmt::Formatter<'a>);

// The documentation recommends implementing std::io::Write,
// but that trait operates on a stream of bytes,
// whereas std::fmt::Write operates on string slices.
// Additionally, we call Formatter::write_str(),
// which returns a Result<(), std::fmt::Error>,
// which matches the signature of std::fmt::Write::write_str().
impl<'a: 'f, 'f> fmt::Write for IndentedWrite<'a, 'f> {
    fn write_str(&mut self, s: &str) -> Result<(), fmt::Error> {
        let mut lines = s.split('\n');
        if let Some(line) = lines.next() {
            self.0.write_str(line)?;
            for line in lines {
                self.0.write_str("\n  ")?;
                self.0.write_str(line)?;
            }
        }

        Ok(())
    }
}
