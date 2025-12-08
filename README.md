# v2x

**Vector to X** is a command-line tool that converts vector graphic images (SVG) into multiple output formats, primarily raster-based formats such as JPEG, PNG, TIFF, WebP, and AVIF.

## Why?

So, when I do graphic editing, mostly I'm doing it using vector-based graphic editor (e.g, [Inkscape](https://inkscape.org/)) and most of the time I will produce multiple output formats along with different dimension size for ease-of-use in different use cases.

My manual approach relies on the graphic editor's built-in export functionality (Certain editor's support command-line mode. However, it's very limited), and for formats that are not supported by the editor itself I will use different utility program like [FFmpeg](https://ffmpeg.org/). As you can see the workflow is too time-consuming and fragmented.

And because of that, this command-line tool is intended to unified and speed up the workflow.

## Usage

```shell
# Show all available options
v2x --help

# Use current working directory as the output directory
v2x ./assets/shape.svg

# Specify output directory
v2x --output ./dist ./assets/shape.svg

# Specify formats. By default generate all formats
v2x --output ./dist --format jpeg,png,avif ./assets/shape.svg

# Set background color
v2x --output ./dist --background "#FB542B" ./assets/shape.svg

# Adjust width
v2x --output ./dist --width 1000 ./assets/shape.svg

# Adjust height
v2x --output ./dist --height 1000 ./assets/shape.svg

# Scale
v2x --output ./dist --scale 1.3 ./assets/shape.svg
```

## Installation

You need rust toolchain to build it yourself

```shell
cargo install --git https://github.com/MarrieMitsu/v2x
```

## Notes

This program does not expose encoders API options; instead, it relies on the default configuration.
