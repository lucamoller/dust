# Dust

Dust is a full-stack web framework that aims to enable quick development of interactive apps written fully in Rust. It aims to allow a developer to write an entire interactive app with relatively few lines of code, specifying both server and frontend logic in a very seamless way. Despite Rust being a complex language (which one could question if it's a good option for quickly writing a web app), the framework tries to do the heavy lifting of managing and hiding a lot of that complexity, allowing the application logic to be written in simple way.

This project is inspired by Plotly Dash: Dash in Rust -> Dust. The main motivation for using Rust is that it can be used to compile client-side code into WebAssembly (WASM), while Dash is usually limited to writing only server-side code in Python. The capability of writing arbitrary client-side logic can be very important when polishing app performance. A secondary motivation is that Rust has a great static typing system, which can add a lot of value in terms of code readability and maintainability.

## Status of the project

The project is in a proof of concept stage. We're trying to figure out if we can implement some minimum feature-set such that it's usable in a concise and simple way.

## Getting started template

Check out [Dust Getting Started](https://github.com/lucamoller/dust-getting-started) for an example of a starting project skeleton. 

## Leptos

[Leptos](https://github.com/leptos-rs/leptos) (another Rust full-stack web framework) is Dust's main dependency. Dust can be seen as an opinionated high level wrapper around it that handles the server-client interactions in a very particular way. Dust expects the client side UI to be defined through Leptos views. 

Some initial early thoughts: We chose Leptos because it seems to provide great server-client integration (Dust's internal callback engine is developed on top it), makes a good effort in taking unnecessary complexity out of the way (with signals, etc) and the [cargo-leptos](https://github.com/akesson/cargo-leptos) tooling makes development pretty straight forward (the concept of separate server binary and wasm builds gets almost completely abstracted away).



