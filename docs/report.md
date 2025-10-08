# Allure Reports

## Overview
`tanu` integrates with the [tanu-allure reporter](https://github.com/tanu-rs/tanu-allure), which emits Allure-compatible JSON for each executed test. The reporter plugs into `tanu_core::Reporter`, so HTTP calls, assertions, and timings automatically flow into Allure dashboards without extra plumbing. See [https://github.com/tanu-rs/tanu-allure](https://github.com/tanu-rs/tanu-allure) for more information.

![](assets/allure-report.png)
