// Copyright 2025 The Oikome Authors
// SPDX-License-Identifier: Apache-2.0

//! `componentize <module.wasm> <component.wasm>`: the module a plugin's
//! source builds to, made the component Oikome runs, and checked.

use std::process::ExitCode;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let [module, component] = args.as_slice() else {
        eprintln!("usage: componentize <module.wasm> <component.wasm>");
        return ExitCode::FAILURE;
    };
    let made = std::fs::read(module)
        .map_err(|e| e.to_string())
        .and_then(|bytes| {
            wit_component::ComponentEncoder::default()
                .module(&bytes)
                .and_then(|encoder| encoder.validate(true).encode())
                .map_err(|e| format!("{e:#}"))
        })
        .and_then(|bytes| std::fs::write(component, bytes).map_err(|e| e.to_string()));
    match made {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("{module}: {e}");
            ExitCode::FAILURE
        }
    }
}
