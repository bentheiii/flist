# Flist
Flist is a software to list files across your computer, it allows for multiple processes to use the same file list, and it allows for easy sharing of files multiple sources.

This app should work on Windows, Linux and MacOS, but it has only been tested on Windows.

## Building

1. Install [Rust](https://www.rust-lang.org/tools/install)
2. Clone this repository
3. Run `cargo build --release`
4. The binary will be in `target/release`, copy and paste in the directory you want your project to be in.

## Usage

1. create a directory to store you project
2. run `flist <directory> new --exit` to create a new project in the directory
3. run `flist <directory>` to view the files in the project
4. run `flist <directory> add <name> <link>` to add a file to the project

The `<directory>` argument is optional, if not provided the current directory will be used.