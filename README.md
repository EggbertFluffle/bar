# Bar

Currently using 

- Smithay client toolkit - Wayland client tools
- Fontdue - Font parsing and glyph rasterizing
- Lua? - configuration and extension

# Ideation

Lua or TOML config

_if_ Lua, it would be mostly a config file returning a table. This leaves some possibilities to use Lua language features, but largely there would be no API for interacting with the bar after the config file is ran.

_if_ TOML, it would just behave like a static config

In the config itself, we should be able to choose the basics:
* font and font size
* window and text padding
* *modules*, where a *module* is one of two things
    * A provided rust module
    * Or a user script

*Rust modules* will provide a way for the bar to have basic features that tap into known notification methods, such as debus or udeb netlinks (idk how these work). These modules include:
    + Date / time
    + PulseAudio / PipeWire / ALSA
    + Battery life / status
    + Focused app name / title
    + Perhaps workspace status for popular compositors

*User scripts* will require a filepath to a executable script. This script should be constnatly running, and should write a new line to stdout when it needs to update visually on the bar.
