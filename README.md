# Feeds to Pocket

<b>Feeds to Pocket</b> watches your RSS and Atom feeds
and pushes new items to your [Pocket][pocket] list.

[pocket]: https://getpocket.com/

## License

<b>Feeds to Pocket</b> is licensed
under the terms of both the [MIT license][license-mit]
and the [Apache License, version 2.0][license-apache].
<b>Feeds to Pocket</b> also uses third party libraries,
some of which have different licenses.

[license-mit]: LICENSE-MIT
[license-apache]: LICENSE-APACHE

## Prerequisites

<b>Feeds to Pocket</b> uses [OpenSSL][openssl] for HTTPS requests.
If you don't have OpenSSL,
you'll have to install it first.

You'll need Cargo, Rust's package manager.
If you don't already have it,
go to the [Rust][rust] home page,
then download and install Rust for your platform,
which will install the Rust compiler and Cargo.

**Note:** <b>Feeds to Pocket</b>
currently requires a *nightly* compiler.

## Usage

### Installation

In a terminal or command prompt,
run the following command:

    $ cargo install feeds-to-pocket

This will install the last version of <b>Feeds to Pocket</b>
that was published to [crates.io][crate].

If you want to install an update, run:

    $ cargo install --force feeds-to-pocket

[openssl]: https://www.openssl.org/
[rust]: https://www.rust-lang.org/
[crate]: https://crates.io/crates/feeds-to-pocket

### Configuration

<b>Feeds to Pocket</b> uses a file to store your configuration
(list of feeds to monitor, Pocket access credentials).
You must specify a file name as a command-line argument
when you call the program;
there's no default file name.

First, you must create your configuration file:

    $ feeds-to-pocket ~/feeds-to-pocket.yaml init

> `~/feeds-to-pocket.yaml` is just an example,
> you can use any file name you want!

Then, you must [create an application][create-app]
on the developer section of Pocket's website.
Make sure you select at least the <b>Add</b> permission.
This will give you a *customer key*,
which is necessary to use Pocket's API.
Customer keys have [rate limits][rate-limits],
so I suggest you keep your customer key private.

When you've obtained your customer key,
save it in your configuration file:

    $ feeds-to-pocket ~/feeds-to-pocket.yaml set-customer-key 1234-abcd1234abcd1234abcd1234

After that, you need to login.
Just run:

    $ feeds-to-pocket ~/feeds-to-pocket.yaml login

and follow the instructions.
This will save an access token in your configuration file.
The access token acts like your account's password,
so keep it safe!

Congratulations, <b>Feeds to Pocket</b> is now ready to talk to Pocket!

### Adding feeds

Once the above configuration steps are done,
you're ready to add feeds.
Use the `add` subcommand to add a feed:

    $ feeds-to-pocket ~/feeds-to-pocket.yaml add https://xkcd.com/atom.xml

This will download the feed
and mark all current entries as "processed"
without sending them to Pocket.
If you would like all current entries to be sent to Pocket,
pass the `--unread` flag:

    $ feeds-to-pocket ~/feeds-to-pocket.yaml add --unread https://xkcd.com/atom.xml

Repeat this for every feed you'd like <b>Feeds to Pocket</b> to monitor.

### Sending new entries to Pocket

Call `feeds-to-pocket` without a subcommand
to have it download your feeds
and send new entries to Pocket.

    $ feeds-to-pocket ~/feeds-to-pocket.yaml

Once an entry has been sent to Pocket,
<b>Feeds to Pocket</b> marks it as "processed"
and will not send it again.

### Assigning tags to feeds

You can assign tags to feeds.
When a new entry is pushed to Pocket,
it will be assigned the tags that were set
on the feed the entry comes from.

To do this, pass the `--tags` option
to the `add` subcommand.
You can do this while adding a new feed
or for an existing feed
(then it will *replace* the list of tags for that feed).
The `--tags` option is followed by a comma-separated list of tags.

    $ feeds-to-pocket ~/feeds-to-pocket.yaml add --tags comics,xkcd https://xkcd.com/atom.xml

### Scheduling

<b>Feeds to Pocket</b> doesn't have any built-in scheduling mechanisms.
You should use an existing task scheduler
to run the `feeds-to-pocket` program periodically.

If you are using Linux with systemd,
you can set up a systemd timer
for your systemd user instance.
See the example unit files in the `systemd-examples` directory.

[create-app]: https://getpocket.com/developer/apps/new
[rate-limits]: https://getpocket.com/developer/docs/rate-limits

## Compiling from source

To build the project, just run:

    $ cargo build

from the project's directory.
This will download and compile
all of the project's Rust dependencies automatically.

## Issues

If you find a bug,
first check if you're using the latest version,
and update if that's not the case.
If the bug still occurs,
please check if there's already a similar [issue][issues]
(check both open and closed issues!).
If there isn't, then [file a new issue][new-issue].
If the program outputs an error message,
please include it in your issue.
Also mention what operating system you're using and which version.

[issues]: https://github.com/FraGag/feeds-to-pocket/issues
[new-issue]: https://github.com/FraGag/feeds-to-pocket/issues/new

## Contributing

See [CONTRIBUTING][contributing].

[contributing]: CONTRIBUTING.md
