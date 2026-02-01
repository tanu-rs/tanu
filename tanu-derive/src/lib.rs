//! # Tanu Derive
//!
//! Procedural macros for the tanu WebAPI testing framework.
//!
//! This crate provides the `#[tanu::test]` and `#[tanu::main]` procedural macros
//! that enable the core functionality of tanu's test discovery and execution system.
//!
//! ## Macros
//!
//! - `#[tanu::test]` - Marks async functions as tanu test cases
//! - `#[tanu::test(param)]` - Creates parameterized test cases  
//! - `#[tanu::main]` - Generates the main function for test discovery
//!
//! These macros are automatically re-exported by the main `tanu` crate,
//! so users typically don't need to import this crate directly.

extern crate proc_macro;

use proc_macro::TokenStream;
use quote::{quote, ToTokens};
use syn::{
    parse::Parse, parse_macro_input, punctuated::Punctuated, spanned::Spanned, Expr, ExprCall,
    ExprLit, ExprPath, Item, ItemFn, ItemMod, Lit, LitStr, ReturnType, Signature, Token, Type,
};

/// Represents arguments in the test attribute #[test(a, b; c)].
struct Input {
    /// Test arguments specified in the test attribute.
    args: Punctuated<Expr, Token![,]>,
    /// Test name specified in the test attribute.
    name: Option<LitStr>,
    /// Serial group name: None = parallel, Some("") = default group, Some("x") = named group
    serial_group: Option<String>,
    /// Whether tests should run in source order (module-level attribute)
    ordered: bool,
}

impl Parse for Input {
    fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
        if input.is_empty() {
            return Ok(Input {
                args: Default::default(),
                name: None,
                serial_group: None,
                ordered: false,
            });
        }

        let mut serial_group: Option<String> = None;
        let mut ordered = false;
        let mut test_args: Punctuated<Expr, Token![,]> = Punctuated::new();

        // Parse all comma-separated arguments, looking for serial
        loop {
            if input.peek(Token![;]) || input.is_empty() {
                break;
            }

            // Check if this is `serial`, `serial = "group"`, or `ordered`
            if input.peek(syn::Ident) {
                let fork = input.fork();
                if let Ok(ident) = fork.parse::<syn::Ident>() {
                    if ident == "serial" {
                        // Consume the serial identifier
                        input.parse::<syn::Ident>()?;

                        // Check for `= "group"`
                        let group = if input.peek(Token![=]) {
                            input.parse::<Token![=]>()?;
                            let lit: LitStr = input.parse()?;
                            Some(lit.value())
                        } else {
                            Some(String::new()) // Empty string for default group
                        };

                        serial_group = group;

                        // Consume comma if present
                        if input.peek(Token![,]) {
                            input.parse::<Token![,]>()?;
                        }
                        continue;
                    } else if ident == "ordered" {
                        // Consume the ordered identifier
                        input.parse::<syn::Ident>()?;
                        ordered = true;

                        // Consume comma if present
                        if input.peek(Token![,]) {
                            input.parse::<Token![,]>()?;
                        }
                        continue;
                    }
                }
            }

            // Not a serial argument, parse as test parameter
            let expr = input.parse::<Expr>()?;
            test_args.push(expr);

            // Consume comma if present
            if input.peek(Token![,]) && !input.peek2(Token![;]) {
                input.parse::<Token![,]>()?;
            } else if !input.peek(Token![;]) && !input.is_empty() {
                break;
            }
        }

        // Parse optional test name after semicolon
        let name = if input.parse::<Token![;]>().is_ok() {
            input.parse::<LitStr>().ok()
        } else {
            None
        };

        Ok(Input {
            args: test_args,
            name,
            serial_group,
            ordered,
        })
    }
}

/// - If a test name argument is provided (e.g., `#[test(a; xxx)]`), use it as the function name.
/// - Otherwise, generate a function name by concatenating the test parameters with `_`.
fn generate_test_name(org_func_name: &str, input: &Input) -> String {
    let func_name = org_func_name.to_string();

    if input.args.is_empty() {
        return func_name.to_string();
    }

    let stringified_args = match &input.name {
        Some(name_argument) => name_argument.value(),
        _ => input
            .args
            .iter()
            .filter_map(|expr| match expr {
                Expr::Lit(ExprLit { lit, .. }) => match lit {
                    Lit::Str(lit_str) => Some(lit_str.value()),
                    other_literal => Some(quote!(#other_literal).to_string()),
                },
                expr @ Expr::Path(_) | expr @ Expr::Call(_) => extract_and_stringify_option(expr),
                other_expr => Some(quote!(#other_expr).to_string()),
            })
            .map(|s| {
                s.replace("+=", "_add_")
                    .replace("+", "_add_")
                    .replace("-=", "_sub_")
                    .replace("-", "_sub_")
                    .replace("/=", "_div_")
                    .replace("/", "_div_")
                    .replace("*=", "_mul_")
                    .replace("*", "_mul_")
                    .replace("%=", "_mod_")
                    .replace("%", "_mod_")
                    .replace("==", "_eq_")
                    .replace("!=", "_nq_")
                    .replace("&&", "_and_")
                    .replace("||", "_or_")
                    .replace("!", "not_")
                    .replace("&=", "_and_")
                    .replace("&", "_and_")
                    .replace("|=", "_or_")
                    .replace("|", "_or_")
                    .replace("^=", "_xor_")
                    .replace("^", "_xor_")
                    .replace("<<=", "_lshift_")
                    .replace("<<", "_lshift_")
                    .replace("<=", "_le_")
                    .replace("<", "_lt_")
                    .replace(">>=", "_rshift_")
                    .replace(">>", "_rshift_")
                    .replace(">=", "_ge_")
                    .replace(">", "_gt_")
                    .replace("&mut ", "")
                    .replace("*mut ", "")
                    .replace("&", "")
                    .replace("*", "")
                    .replace(" :: ", "_")
                    .replace("\\", "")
                    .replace("/", "")
                    .replace("\"", "")
                    .replace("(", "")
                    .replace(")", "")
                    .replace("{", "")
                    .replace("}", "")
                    .replace("[", "")
                    .replace("]", "")
                    .replace(" ", "")
                    .replace(",", "_")
                    .replace(".", "_")
                    .to_lowercase()
            })
            .collect::<Vec<_>>()
            .join("_"),
    };

    format!("{func_name}::{stringified_args}")
}

#[derive(Debug, Eq, PartialEq)]
enum ErrorCrate {
    Eyre,
    AnythingElse,
}

/// Inspects the current function's signature to determine which error crate is being used.
///
/// This function analyzes the return type of the function to detect whether it is using
/// `eyre::Result` or another error result type. It then enables conditional handling based
/// on the error crate in use (e.g., wrapping non-`eyre::Result` types in an `eyre::Result`).
///
/// **Limitation:**
/// Due to the inherent limitations of proc macros, this function can only detect error types
/// when `eyre` is referenced using its fully qualified path (for example, `eyre::Result`).
///
/// For further details and discussion on this limitation, see:
/// https://users.rust-lang.org/t/in-a-proc-macro-attribute-procedural-macro-how-to-get-the-full-typepath-of-some-type/107713/2
fn inspect_error_crate(sig: &Signature) -> ErrorCrate {
    match &sig.output {
        ReturnType::Default => panic!("return type needs to be other than ()"),
        ReturnType::Type(_, ty) => {
            let Type::Path(type_path) = ty.as_ref() else {
                panic!("failed to get return type path");
            };

            let path = &type_path.path;
            match (path.segments.first(), path.segments.last()) {
                (Some(first), Some(last)) => {
                    if first.ident == "eyre" && last.ident == "Result" {
                        ErrorCrate::Eyre
                    } else {
                        ErrorCrate::AnythingElse
                    }
                }
                _ => {
                    panic!("unexpected return type");
                }
            }
        }
    }
}

#[allow(dead_code)]
/// Returns the name of the variant of the given expression.
fn get_expr_variant_name(expr: &Expr) -> &'static str {
    match expr {
        Expr::Array(_) => "Array",
        Expr::Assign(_) => "Assign",
        Expr::Async(_) => "Async",
        Expr::Await(_) => "Await",
        Expr::Binary(_) => "Binary",
        Expr::Block(_) => "Block",
        Expr::Break(_) => "Break",
        Expr::Call(_) => "Call",
        Expr::Cast(_) => "Cast",
        Expr::Closure(_) => "Closure",
        Expr::Continue(_) => "Continue",
        Expr::Field(_) => "Field",
        Expr::ForLoop(_) => "ForLoop",
        Expr::Group(_) => "Group",
        Expr::If(_) => "If",
        Expr::Index(_) => "Index",
        Expr::Let(_) => "Let",
        Expr::Lit(_) => "Lit",
        Expr::Loop(_) => "Loop",
        Expr::Macro(_) => "Macro",
        Expr::Match(_) => "Match",
        Expr::MethodCall(_) => "MethodCall",
        Expr::Paren(_) => "Paren",
        Expr::Path(_) => "Path",
        Expr::Range(_) => "Range",
        Expr::Reference(_) => "Reference",
        Expr::Repeat(_) => "Repeat",
        Expr::Return(_) => "Return",
        Expr::Struct(_) => "Struct",
        Expr::Try(_) => "Try",
        Expr::TryBlock(_) => "TryBlock",
        Expr::Tuple(_) => "Tuple",
        Expr::Unary(_) => "Unary",
        Expr::Unsafe(_) => "Unsafe",
        Expr::Verbatim(_) => "Verbatim",
        Expr::While(_) => "While",
        Expr::Yield(_) => "Yield",
        _ => "Unknown",
    }
}

fn extract_and_stringify_option(expr: &Expr) -> Option<String> {
    match expr {
        Expr::Call(ExprCall { func, args, .. }) => {
            if let Expr::Path(ExprPath { path, .. }) = &**func {
                let segment = path.segments.last()?;
                if segment.ident == "Some" {
                    match args.first()? {
                        Expr::Lit(ExprLit { lit, .. }) => match lit {
                            Lit::Str(lit_str) => {
                                return Some(lit_str.value());
                            }
                            other_type_of_literal => {
                                return Some(other_type_of_literal.to_token_stream().to_string());
                            }
                        },
                        first_arg => {
                            return Some(quote!(#first_arg).to_string());
                        }
                    }
                }
            }
        }
        Expr::Path(ExprPath { path, .. }) => {
            if path.get_ident()? == "None" {
                return Some("None".into());
            }
        }
        _ => {}
    }

    None
}

/// Handles #[tanu::test(ordered)] when applied to a module.
/// Injects 'ordered' parameter into all #[tanu::test] attributes within the module.
fn handle_ordered_module(mut module: ItemMod) -> TokenStream {
    // Process the module contents if present
    if let Some((_, items)) = &mut module.content {
        for item in items.iter_mut() {
            if let Item::Fn(func) = item {
                // Check if this function has #[tanu::test] attribute
                let has_tanu_test = func.attrs.iter().any(|attr| {
                    if let Some(segment) = attr.path().segments.first() {
                        segment.ident == "tanu"
                    } else {
                        false
                    }
                });

                if has_tanu_test {
                    // Find and modify the #[tanu::test] attribute
                    for attr in func.attrs.iter_mut() {
                        if let Some(segment) = attr.path().segments.first() {
                            if segment.ident == "tanu" {
                                // Preserve original attribute span so line!() stays accurate
                                let attr_span = attr.span();
                                // Parse the existing attribute arguments
                                let tokens = attr.meta.require_list().ok().map(|list| {
                                    let tokens = &list.tokens;
                                    tokens.clone()
                                });

                                // Reconstruct with ordered added
                                let new_tokens = if let Some(existing) = tokens {
                                    quote::quote_spanned! { attr_span => ordered, #existing }
                                } else {
                                    quote::quote_spanned! { attr_span => ordered }
                                };

                                // Replace the attribute
                                *attr = syn::parse_quote_spanned! { attr_span =>
                                    #[tanu::test(#new_tokens)]
                                };
                            }
                        }
                    }
                }
            }
        }
    }

    quote! { #module }.into()
}

/// Marks an async function as a tanu test case.
///
/// This attribute registers the function with tanu's test discovery system,
/// making it available for execution via the test runner.
///
/// # Basic Usage
///
/// ```rust,ignore
/// #[tanu::test]
/// async fn my_test() -> eyre::Result<()> {
///     // Test implementation
///     Ok(())
/// }
/// ```
///
/// # Parameterized Tests
///
/// The macro supports parameterized testing by accepting arguments:
///
/// ```rust,ignore
/// #[tanu::test(200)]
/// #[tanu::test(404)]
/// #[tanu::test(500)]
/// async fn test_status_codes(status: u16) -> eyre::Result<()> {
///     // Test with different status codes
///     Ok(())
/// }
/// ```
///
/// # Requirements
///
/// - Function must be `async`
/// - Function must return a `Result<T, E>` type
/// - Supported Result types: `eyre::Result`, `anyhow::Result`, `std::result::Result`
///
/// # Error Handling
///
/// The macro automatically handles different Result types and integrates
/// with tanu's error reporting system for enhanced error messages and backtraces.
#[proc_macro_attribute]
pub fn test(args: TokenStream, input: TokenStream) -> TokenStream {
    let input_args = parse_macro_input!(args as Input);

    // Try to parse as module first (for #[tanu::test(ordered)] on modules)
    if let Ok(module) = syn::parse::<ItemMod>(input.clone()) {
        if input_args.ordered {
            return handle_ordered_module(module);
        }
        // If it's a module but not ordered, return error
        return syn::Error::new_spanned(
            module,
            "#[tanu::test] on modules requires 'ordered' parameter. Use #[tanu::test(ordered)]",
        )
        .to_compile_error()
        .into();
    }

    // Parse as function
    let input_fn = parse_macro_input!(input as ItemFn);

    let func_name_inner = &input_fn.sig.ident;
    let test_name_str = generate_test_name(&func_name_inner.to_string(), &input_args);

    let args = input_args.args.to_token_stream();

    // Generate serial_group token
    // When ordered is true, auto-create serial group based on module path
    let serial_group_tokens = if input_args.ordered {
        quote! { Some(module_path!()) }
    } else {
        match &input_args.serial_group {
            None => quote! { None },
            Some(s) if s.is_empty() => quote! { Some("") },
            Some(s) => quote! { Some(#s) },
        }
    };

    let ordered = input_args.ordered;

    // tanu internally relies on the `eyre` and `color-eyre` crates for error handling.
    // since `tanu::Runner` expects test functions to return an `eyre::Result`, the macro
    // generates two types of code.
    //
    // - If a test function explicitly returns `eyre::Result`, the macro will generate
    //   a function that also returns `eyre::Result` without modification.
    //
    // - If the test function returns another result type (e.g., `anyhow::Result`),
    //   the macro will automatically wrap the return value in an `eyre::Result`.
    let error_crate = inspect_error_crate(&input_fn.sig);
    let output = if error_crate == ErrorCrate::Eyre {
        quote! {
            #input_fn

            // Submit test to inventory for discovery
            ::tanu::inventory::submit! {
                ::tanu::TestRegistration {
                    module: module_path!(),
                    name: #test_name_str,
                    serial_group: #serial_group_tokens,
                    line: line!(),
                    ordered: #ordered,
                    test_fn: || {
                        Box::pin(async move {
                            #func_name_inner(#args).await
                        })
                    },
                }
            }
        }
    } else {
        quote! {
            #input_fn

            // Submit test to inventory for discovery
            ::tanu::inventory::submit! {
                ::tanu::TestRegistration {
                    module: module_path!(),
                    name: #test_name_str,
                    serial_group: #serial_group_tokens,
                    line: line!(),
                    ordered: #ordered,
                    test_fn: || {
                        Box::pin(async move {
                            #func_name_inner(#args).await.map_err(|e| ::tanu::eyre::eyre!(Box::new(e)))
                        })
                    },
                }
            }
        }
    };

    output.into()
}

/// Generates the test discovery and registration code for tanu.
///
/// This attribute should be applied to your main function alongside `#[tokio::main]`.
/// It automatically discovers all functions marked with `#[tanu::test]` and registers
/// them with the test runner.
///
/// # Usage
///
/// ```rust,ignore
/// #[tanu::main]
/// #[tokio::main]
/// async fn main() -> eyre::Result<()> {
///     let runner = run();
///     let app = tanu::App::new();
///     app.run(runner).await?;
///     Ok(())
/// }
/// ```
///
/// # What It Does
///
/// The macro performs compile-time test discovery by:
/// 1. Scanning the codebase for `#[tanu::test]` annotated functions
/// 2. Generating a `run()` function that returns a configured `Runner`
/// 3. Registering all discovered tests with the runner
/// 4. Setting up proper module organization and test metadata
///
/// # Requirements
///
/// - Must be used with `#[tokio::main]` for async support
/// - The main function should return a `Result` type
/// - All test functions must be marked with `#[tanu::test]`
///
/// # Generated Code
///
/// The macro generates a `run()` function that you can use to obtain
/// a pre-configured test runner with all your tests registered.
#[proc_macro_attribute]
pub fn main(_args: TokenStream, input: TokenStream) -> TokenStream {
    let main_fn = parse_macro_input!(input as ItemFn);

    let output = quote! {
        fn run() -> tanu::Runner {
            let mut runner = tanu::Runner::new();

            // Use inventory to discover all registered tests
            for test in ::tanu::inventory::iter::<::tanu::TestRegistration> {
                runner.add_test(
                    test.name,
                    test.module,
                    test.serial_group,
                    test.line,
                    test.ordered,
                    std::sync::Arc::new(test.test_fn)
                );
            }

            runner
        }

        #main_fn
    };

    output.into()
}

#[cfg(test)]
mod test {
    use crate::Input;

    use super::{ErrorCrate, Expr};
    use test_case::test_case;

    #[test_case("fn foo() -> eyre::Result" => ErrorCrate::Eyre; "eyre")]
    #[test_case("fn foo() -> anyhow::Result" => ErrorCrate::AnythingElse; "anyhow")]
    #[test_case("fn foo() -> miette::Result" => ErrorCrate::AnythingElse; "miette")]
    #[test_case("fn foo() -> Result" => ErrorCrate::AnythingElse; "std_result")]
    fn inspect_error_crate(s: &str) -> ErrorCrate {
        let sig: syn::Signature = syn::parse_str(s).expect("failed to parse function signature");
        super::inspect_error_crate(&sig)
    }

    #[test_case("Some(1)" => Some("1".into()); "Some with int")]
    #[test_case("Some(\"test\")" => Some("test".into()); "Some with string")]
    #[test_case("Some(true)" => Some("true".into()); "Some with boolean")]
    #[test_case("Some(1.0)" => Some("1.0".into()); "Some with float")]
    #[test_case("Some(StatusCode::OK)" => Some("StatusCode :: OK".into()); "Some third party type")]
    #[test_case("Some(\"foo\".to_string())" => Some("\"foo\" . to_string ()".into()); "Some expression")]
    #[test_case("None" => Some("None".into()); "None")]
    fn extract_and_stringify_option(s: &str) -> Option<String> {
        let expr: Expr = syn::parse_str(s).expect("failed to parse expression");
        super::extract_and_stringify_option(&expr)
    }

    #[allow(clippy::erasing_op)]
    #[test_case("a, b; \"test_name\"" => "foo::test_name"; "with test name")]
    #[test_case("1+1" => "foo::1_add_1"; "with add expression")]
    #[test_case("1+=1" => "foo::1_add_1"; "with add assignment expression")]
    #[test_case("1-1" => "foo::1_sub_1"; "with sub expression")]
    #[test_case("1-=1" => "foo::1_sub_1"; "with sub assignment expression")]
    #[test_case("1/1" => "foo::1_div_1"; "with div expression")]
    #[test_case("1/=1" => "foo::1_div_1"; "with div assignment expression")]
    #[test_case("1*1" => "foo::1_mul_1"; "with mul expression")]
    #[test_case("1*=1" => "foo::1_mul_1"; "with mul assignment expression")]
    #[test_case("1%1" => "foo::1_mod_1"; "with mod expression")]
    #[test_case("1%=1" => "foo::1_mod_1"; "with mod assignment expression")]
    #[test_case("1==1" => "foo::1_eq_1"; "with eq expression")]
    #[test_case("1!=1" => "foo::1_nq_1"; "with neq expression")]
    #[test_case("1<1" => "foo::1_lt_1"; "with lt expression")]
    #[test_case("1>1" => "foo::1_gt_1"; "with gt expression")]
    #[test_case("1<=1" => "foo::1_le_1"; "with le expression")]
    #[test_case("1>=1" => "foo::1_ge_1"; "with ge expression")]
    #[test_case("true&&false" => "foo::true_and_false"; "with and expression")]
    #[test_case("true||false" => "foo::true_or_false"; "with or expression")]
    #[test_case("!true" => "foo::not_true"; "with not expression")]
    #[test_case("1&1" => "foo::1_and_1"; "with bitwise and expression")]
    #[test_case("1&=1" => "foo::1_and_1"; "with bitwise and assignment expression")]
    #[test_case("1|1" => "foo::1_or_1"; "with bitwise or expression")]
    #[test_case("1|=1" => "foo::1_or_1"; "with bitwise or assignment expression")]
    #[test_case("1^1" => "foo::1_xor_1"; "with xor expression")]
    #[test_case("1^=1" => "foo::1_xor_1"; "with xor assignment expression")]
    #[test_case("1<<1" => "foo::1_lshift_1"; "with left shift expression")]
    #[test_case("1<<=1" => "foo::1_lshift_1"; "with left shift assignment expression")]
    #[test_case("1>>1" => "foo::1_rshift_1"; "with right shift expression")]
    #[test_case("1>>=1" => "foo::1_rshift_1"; "with right shift assignment expression")]
    #[test_case("\"bar\".to_string()" => "foo::bar_to_string"; "to_string")]
    #[test_case("1+1*2" => "foo::1_add_1_mul_2"; "with add and mul expression")]
    #[test_case("1*(2+3)" => "foo::1_mul_2_add_3"; "with mul and add expression")]
    #[test_case("1+2-3" => "foo::1_add_2_sub_3"; "with add and sub expression")]
    #[test_case("1/2*3" => "foo::1_div_2_mul_3"; "with div and mul expression")]
    #[test_case("1%2+3" => "foo::1_mod_2_add_3"; "with mod and add expression")]
    #[test_case("1==2&&3!=4" => "foo::1_eq_2_and_3_nq_4"; "with eq and and expression")]
    #[test_case("true||false&&true" => "foo::true_or_false_and_true"; "with or and and expression")]
    #[test_case("!(1+2)" => "foo::not_1_add_2"; "with not and add expression")]
    #[test_case("1&2|3^4" => "foo::1_and_2_or_3_xor_4"; "with bitwise and, or, xor expression")]
    #[test_case("1<<2>>3" => "foo::1_lshift_2_rshift_3"; "with left shift and right shift expression")]
    #[test_case("Some(1+2)" => "foo::1_add_2"; "with Some and add expression")]
    #[test_case("None" => "foo::none"; "with None")]
    #[test_case("[1, 2]" => "foo::1_2"; "with array")]
    #[test_case("vec![1, 2]" => "foo::vecnot_1_2"; "with macro")] // TODO should parse macro so that it won't have "not"
    #[test_case("\"foo\".to_string().len()" => "foo::foo_to_string_len"; "with function call chain")]
    #[test_case("0.5+0.3" => "foo::0_5_add_0_3"; "with floating point add")] // TODO should be foo::05_add_03
    #[test_case("-10" => "foo::_sub_10"; "with negative number")] // TODO should be neg_10
    #[test_case("1.0e10" => "foo::1_0e10"; "with scientific notation")] // TODO should be foo::10e10
    #[test_case("0xff" => "foo::0xff"; "with hex literal")]
    #[test_case("0o777" => "foo::0o777"; "with octal literal")]
    #[test_case("0b1010" => "foo::0b1010"; "with binary literal")]
    #[test_case("\"hello\" + \"world\"" => "foo::hello_add_world"; "with string concatenation")]
    #[test_case("format!(\"{}{}\", 1, 2)" => "foo::formatnot__1_2"; "with format macro")] // TODO should be format_1_2
    #[test_case("r#\"raw string\"#" => "foo::rawstring"; "with raw string")]
    //#[test_case("\n\t\r" => "foo::n_t_r"; "with escape sequences")] // TODO this does not work yet
    #[test_case("(1, \"hello\", true)" => "foo::1_hello_true"; "with mixed tuple")]
    //#[test_case("HashSet::from([1, 2, 3])" => "foo::hashsetfrom_1_2_3"; "with collection construction")] // TODO should be 1_2_3
    //#[test_case("add(1, 2)" => "foo::add1_2"; "with function call")] // This does not work
    //#[test_case("HashSet::from([1, 2, 3])" => "foo::hashsetfrom_1_2_3"; "with collection construction")] // TODO should be 1_2_3
    #[test_case("vec![1..5]" => "foo::vecnot_1__5"; "with range in macro")]
    //#[test_case("add(1, 2)" => "foo::add1_2"; "with function call")] // This does not work
    #[test_case("x.map(|v| v+1)" => "foo::x_map_or_v_or_v_add_1"; "with closure")]
    #[test_case("a.into()" => "foo::a_into"; "with method call no args")]
    // should be a_parse_i32_unwrap
    #[test_case("a.parse::<i32>().unwrap()" => "foo::a_parse__lt_i32_gt__unwrap"; "with turbofish syntax")]
    // #[test_case("if x { 1 } else { 2 }" => "foo::if_x_1_else_2"; "with if expression")]
    // #[test_case("match x { Some(v) => v, None => 0 }" => "foo::match_x_somev_v_none_0"; "with match expression")]
    //#[test_case("Box::new(1)" => "foo::boxnew_1"; "with box allocation")]
    //#[test_case("Rc::new(vec![1, 2])" => "foo::rcnew_vecnot_1_2"; "with reference counting")]
    //#[test_case("<Vec<i32> as IntoIterator>::into_iter" => "foo::veci32_as_intoiterator_into_iter"; "with type casting")]
    // TODO should be 1_10
    #[test_case("1..10" => "foo::1__10"; "with range")]
    //#[test_case("1..=10" => "foo::1_10"; "with inclusive range")]
    //#[test_case("..10" => "foo::_10"; "with range to")]
    //#[test_case("10.." => "foo::10_"; "with range from")]
    fn generate_test_name(args: &str) -> String {
        let input_args: Input = syn::parse_str(args).expect("failed to parse input args");
        super::generate_test_name("foo", &input_args)
    }
}
