#![feature(custom_derive)]
#![feature(plugin)]
#![plugin(serde_macros)]

#[macro_use]
extern crate clap;
extern crate hyper;
extern crate mime;
#[macro_use]
extern crate quick_error;
extern crate serde;
extern crate serde_json;
extern crate serde_yaml;
extern crate syndication;
extern crate url;

mod pocket;

use std::error::Error;
use std::fmt::{self, Display};
use std::fs::{self, File};
use std::io::{self, Read, Write};
use std::path::Path;
use std::process;
use clap::{App, Arg, ArgMatches, SubCommand};
use pocket::Pocket;

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
            .about("Adds a feed to your feeds configuration.")
            .arg(Arg::with_name(subcommands::add::args::UNREAD)
                .long("--unread")
                .help("Consider all the entries in the feed to be unread. \
                       All entries will be sent to Pocket immediately. \
                       By default, all the entries present when the feed is added \
                       are considered read and are not sent to Pocket."))
            .arg(Arg::with_name(subcommands::add::args::FEED_URL)
                .help("The URL of the feed to add.")
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
            pub const FEED_URL: &'static str = "feed url";
        }
    }
}

fn run(args: &ArgMatches) -> Result<(), ErrorWithContext> {
    let mut config = try!(load_config(args));

    // Dispatch based on the subcommand
    try!(match args.subcommand() {
        ("", _) => sync(&mut config),
        (subcommands::set_consumer_key::NAME, Some(args)) => Ok(set_consumer_key(&mut config, &args)),
        (subcommands::login::NAME, _) => login(&mut config),
        (subcommands::add::NAME, Some(args)) => add(&mut config, &args),
        (_, _) => unreachable!(),
    });

    save_config(&config, args)
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
    try!(rename(config_file_name, old_config_file_name));

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

fn set_consumer_key(config: &mut Configuration, args: &ArgMatches) {
    config.consumer_key = args.value_of(subcommands::set_consumer_key::args::KEY).map(String::from);
}

fn login(config: &mut Configuration) -> Result<(), ErrorWithContext> {
    let mut pocket = try_with_context!(get_pocket(config), "unable to perform authorization");

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
    let mut pocket = try_with_context!(get_authenticated_pocket(config), "unable to sync");

    for feed in &mut config.feeds {
        process_feed(feed, Some(&mut pocket)).unwrap_or_else(|e| {
            let _ = writeln!(io::stderr(), "{}", e);
        });
    }

    Ok(())
}

fn add(config: &mut Configuration, args: &ArgMatches) -> Result<(), ErrorWithContext> {
    let send_to_pocket = args.is_present(subcommands::add::args::UNREAD);
    let mut pocket = if send_to_pocket {
        Some(try_with_context!(get_authenticated_pocket(config), "unable to add feed"))
    } else {
        None
    };

    let feed_url = args.value_of(subcommands::add::args::FEED_URL).unwrap();
    if config.feeds.iter().any(|feed| feed.url == feed_url) {
        println!("This feed is already in your configuration!");
        return Ok(());
    }

    config.feeds.push(Feed {
        url: String::from(feed_url),
        processed_entries: vec![],
    });

    let feed = config.feeds.last_mut().unwrap();

    process_feed(feed, pocket.as_mut())
}

fn get_pocket(config: &Configuration) -> Result<Pocket, PocketSetupError> {
    match config.consumer_key {
        Some(ref consumer_key) => Ok(Pocket::new(consumer_key, config.access_token.as_ref().map(|x| x.as_ref()))),
        None => Err(PocketSetupError::MissingConsumerKey),
    }
}

fn get_authenticated_pocket(config: &Configuration) -> Result<Pocket, PocketSetupError> {
    get_pocket(config).and_then(|pocket| {
        match config.access_token {
            Some(_) => Ok(pocket),
            None => Err(PocketSetupError::MissingAccessToken),
        }
    })
}

fn process_feed(feed: &mut Feed, mut pocket: Option<&mut Pocket>) -> Result<(), ErrorWithContext> {
    println!("downloading {}", feed.url);
    let feed_body = try_with_context!(fetch(feed),
        format!("failed to download feed at {url}", url=feed.url));

    let parsed_feed = try_with_context!(feed_body.parse::<syndication::Feed>(),
        format!("failed to parse feed at {url} as either RSS or Atom", url=feed.url));

    let (mut rss_entries, mut atom_entries);
    let entries: &mut Iterator<Item=String> = match parsed_feed {
        syndication::Feed::RSS(rss) => {
            rss_entries = rss.items.into_iter().rev().flat_map(|item| item.link);
            &mut rss_entries
        }
        syndication::Feed::Atom(atom) => {
            atom_entries = atom.entries.into_iter().rev().flat_map(|entry| entry.links).map(|link| link.href);
            &mut atom_entries
        }
    };

    for entry_url in entries {
        // The rss and atom_syndication libraries
        // don't trim the values extracted from the XML files.
        let entry_url = trim(entry_url);

        // Ignore entries we've processed previously.
        if !feed.processed_entries.iter().rev().any(|x| x == &entry_url) {
            let is_processed =
                if let Some(ref mut pocket) = pocket {
                    // Push the entry to Pocket.
                    // Only consider the entry processed if the push succeeded.
                    // That means that if it failed, we'll try again next time.
                    println!("pushing {} to Pocket", entry_url);
                    let push_result = pocket.push(&entry_url);
                    match push_result {
                        Ok(_) => true,
                        Err(error) => {
                            println!("error while adding URL {url} to Pocket:\n  {error}",
                                url=entry_url, error=Indented(&error));
                            false
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
                feed.processed_entries.push(entry_url);
            }
        }
    }

    Ok(())
}

fn fetch(feed: &Feed) -> Result<String, ErrorWithContext> {
    let mut client = hyper::Client::new();
    client.set_redirect_policy(hyper::client::RedirectPolicy::FollowAll);
    let mut response = try_with_context!(client.get(&feed.url)
        .header(hyper::header::UserAgent(String::from(concat!("feeds-to-pocket/", env!("CARGO_PKG_VERSION")))))
        .send(),
        "failed to send request");
    if !response.status.is_success() {
        try_with_context!(Err(UnacceptableHttpStatus::UnacceptableHttpStatus(response.status)),
            format!("the HTTP request to <{}> didn't return a success status", feed.url));
    }

    let mut body = String::new();
    try_with_context!(response.read_to_string(&mut body),
        "failed to read response");
    Ok(body)
}

fn trim(s: String) -> String {
    // This implementation only allocates if the string isn't already trimmed.
    {
        let trimmed = s.trim();
        let is_already_trimmed =
            trimmed.as_ptr() == s.as_ptr() &&
            trimmed.len() == s.len();
        if is_already_trimmed {
            None // can't use `s` here, because it's borrowed by `trimmed`
        } else {
            Some(String::from(trimmed))
        }
    }.unwrap_or(s)
}

#[derive(Deserialize, Serialize)]
struct Configuration {
    #[serde(skip_serializing_if="Option::is_none")]
    consumer_key: Option<String>,
    #[serde(skip_serializing_if="Option::is_none")]
    access_token: Option<String>,
    #[serde(skip_serializing_if="Vec::is_empty")]
    #[serde(default)]
    feeds: Vec<Feed>,
}

#[derive(Deserialize, Serialize)]
struct Feed {
    url: String,
    #[serde(skip_serializing_if="Vec::is_empty")]
    #[serde(default)]
    processed_entries: Vec<String>,
}

#[derive(Debug)]
struct ErrorWithContext {
    error: Box<Error>,
    context: String
}

impl ErrorWithContext {
    fn new<S: Into<String>>(error: Box<Error>, context: S) -> ErrorWithContext {
        ErrorWithContext {
            error: error,
            context: context.into(),
        }
    }
}

impl Display for ErrorWithContext {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> Result<(), fmt::Error> {
        write!(fmt, "{}:\n  {}", self.context, Indented(&self.error))
    }
}

impl Error for ErrorWithContext {
    fn description(&self) -> &str {
        &self.context
    }

    fn cause(&self) -> Option<&Error> {
        Some(&*self.error)
    }
}

quick_error! {
    #[cfg_attr(feature="clippy", allow(enum_variant_names))]
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
        UnacceptableHttpStatus(status: hyper::status::StatusCode) {
            display("{}", status)
        }
    }
}

quick_error! {
    #[derive(Debug)]
    enum Errors {
        Errors(errors: Vec<Box<Error>>) {
            description("Multiple errors occurred.")
            display("{}", errors.iter().map(|error| format!("- {}", Indented(error))).collect::<Vec<_>>().join("\n"))
        }
    }
}

impl Errors {
    fn new(errors: Vec<Box<Error>>) -> Errors {
        Errors::Errors(errors)
    }
}

/// Wraps a type implementing Display
/// and adds two spaces after each line feed in its display output.
struct Indented<'a, D: Display + 'a>(&'a D);

impl<'a, D: Display + 'a> Display for Indented<'a, D> {
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
            try!(self.0.write_str(line));
            for line in lines {
                try!(self.0.write_str("\n  "));
                try!(self.0.write_str(line));
            }
        }

        Ok(())
    }
}
