<p align="center"><img src="assets/tanu.png" width=180></p>
<p align="center"><b>tanu</b>: high-performance, async-friendly and ergonomic WebAPI testing framework for Rust</p>
<p align="center"><img src="https://img.shields.io/crates/v/tanu"/> <img src="https://img.shields.io/crates/l/tanu"/> <img src="https://docs.rs/tanu/badge.svg"/></p>

## Motivation

As a long time backend engineer, I have always been passionate about building reliable and efficient systems. When working with WebAPIs, ensuring correctness, stability, and performance is crucial, yet I often found existing testing frameworks lacking in speed, flexibility, or Rust-native support. This led me to create a WebAPI testing framework in Rust.

While some WebAPI testing tools exist for Rust, they often lack ergonomics, are too low-level, or don't integrate well with modern Rust web frameworks. My goal was to create a framework that is:
* **Fast and lightweight** – Leveraging Rust’s zero-cost abstractions to minimize unnecessary overhead.
* **Type-safe and ergonomic** – Taking advantage of Rust’s strong type system to prevent common errors at compile time.
* **Easily extensible** – Allowing developers to integrate custom assertions, mocking, and performance metrics seamlessly.
* **Concurrency and async-friendly** – Supporting asynchronous requests and concurrent execution to test APIs efficiently.

I tried multiple solutions in the past but encountered significant limitations:
* **Postman** - Postman is a great tool but not designed for API end-to-end testing. You need a GUI and have to write assertions in JavaScript, which results in massive JSON files that become difficult to manage.
* **Playwright** - Playwright is an excellent framework for web end-to-end testing. While it does support API testing, I wanted to use the same language for both API implementation and tests, which Playwright does not offer.
* **Rust Standard Test Framework** - I attempted multiple times to write API tests using `#[test]`, along with [tokio](https://crates.io/crates/tokio), [test-case](https://crates.io/crates/test-case), and [reqwest](https://crates.io/crates/reqwest) crates. While functional, this approach lacked structure and ergonomics for writing effective tests at scale. I wanted a dedicated framework to simplify and streamline the process.
