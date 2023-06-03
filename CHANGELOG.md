# Changelog

## 0.1.7 - 2023-06-03

- Fix incorrect instructions in README.md (PR #5)
- Update dependencies
- General code cleanup

## 0.1.6 - 2018-11-10

- Update dependencies, enabling support for OpenSSL 1.1.1
- Add the 'remove' subcommand to remove a feed from a configuration file
  (issue #2)

## 0.1.5 - 2017-12-10

- Update dependencies, including notably updating `atom_syndication` to
  0.5, which is more lenient in parsing some invalid Atom feeds (e.g.
  feeds that are missing `<updated>` elements)

## 0.1.4 - 2017-09-03

- Update dependencies

## 0.1.3 - 2016-12-18

- Ignore invalid URLs in feeds (instead of panicking)
- Update dependencies

## 0.1.2 - 2016-09-20

- Update dependencies, including notably updating `atom_syndication` to
  0.2, which prevents feeds-to-pocket from panicking when decoding some
  invalid feeds

## 0.1.1 - 2016-07-24

- Support compiling with the stable Rust compiler
- Update dependencies

## 0.1.0 - 2016-06-26

Initial release.
