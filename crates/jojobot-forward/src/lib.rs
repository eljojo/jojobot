//! **A test double names the methods it changes; the answer of the rest is
//! generated from the port.**
//!
//! Three pieces, in the order they run:
//!
//! 1. [`port`] sits on a port trait. It leaves the trait as it is and
//!    writes a `macro_rules!` beside it that carries the trait's method list.
//! 2. [`forward`] sits on the `impl` of a double. It hands the impl to that macro.
//! 3. [`__finish`] receives the method list and the impl together, and adds a
//!    method for each port method the impl does not write.
//!
//! The callback through a `macro_rules!` is how the impl sees a trait that is
//! declared in another crate: a proc macro cannot read another item, but a macro
//! the trait wrote can carry its method list to one.
//!
//! **What an added method does** depends on the mode the double asks for:
//!
//! - `via = <field>`: `self.<field>.<method>(<args>)`, awaited when the port
//!   method is async. The double wraps a store and changes some methods.
//! - `down = <expr>`: `Err(<expr>)`, for each method that returns a `Result`. The
//!   double answers nothing, and says with one error how it fails.
//!
//! `required_only` leaves each method that has a default body to that body, and
//! `also(<method>, ...)` names the defaulted methods that are added anyway. A
//! method the impl writes is left as it was written.

use proc_macro::TokenStream;
use proc_macro2::{Span, TokenStream as TokenStream2, TokenTree};
use quote::{format_ident, quote};
use syn::parse::{Parse, ParseStream};
use syn::{
    Expr, FnArg, Ident, ImplItem, ImplItemFn, ItemImpl, ItemTrait, LitStr, Member, Pat, Path,
    ReturnType, Token, TraitItem, TraitItemFn, Type, braced, parse_macro_input,
};

/// The arguments of [`port`]: `callback = <name>, scope = "<module>"`.
struct PortArgs {
    name: Ident,
    scope: String,
    uses: Vec<Path>,
}

impl Parse for PortArgs {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let mut name = None;
        let mut scope = None;
        let mut uses = Vec::new();
        while !input.is_empty() {
            let key: Ident = input.parse()?;
            if key == "uses" {
                let paths;
                syn::parenthesized!(paths in input);
                uses = paths
                    .parse_terminated(Path::parse_mod_style, Token![,])?
                    .into_iter()
                    .collect();
                if !input.is_empty() {
                    input.parse::<Token![,]>()?;
                }
                continue;
            }
            input.parse::<Token![=]>()?;
            if key == "callback" {
                name = Some(input.parse::<Ident>()?);
            } else if key == "scope" {
                scope = Some(input.parse::<LitStr>()?.value());
            } else {
                return Err(syn::Error::new(
                    key.span(),
                    "expected `callback`, `scope` or `uses(<path>, ...)`",
                ));
            }
            if !input.is_empty() {
                input.parse::<Token![,]>()?;
            }
        }
        Ok(Self {
            name: name.ok_or_else(|| input.error("`callback = <name>` is required"))?,
            scope: scope.ok_or_else(|| input.error("`scope = \"<module>\"` is required"))?,
            uses,
        })
    }
}

/// **Put a port's method list where a double can reach it.** Written on the trait:
/// `#[port(callback = forward_memory, scope = "memory", uses(::jiff::civil::Date))]`.
/// The trait is emitted unchanged. Beside it comes an exported macro of the given
/// name, which is what [`forward`] calls; `scope` is the module of the crate that
/// declares the trait, whose public names the added signatures are read in, and
/// `uses` names the other paths the signatures spell without a prefix (the
/// standard maps and `Arc` are always in scope).
#[proc_macro_attribute]
pub fn port(args: TokenStream, item: TokenStream) -> TokenStream {
    let args = parse_macro_input!(args as PortArgs);
    let port = parse_macro_input!(item as ItemTrait);
    let signatures: Vec<TokenStream2> = port
        .items
        .iter()
        .filter_map(|item| match item {
            TraitItem::Fn(method) => {
                let sig = &method.sig;
                let marked = match method.default {
                    Some(_) => quote! { #[has_default] },
                    None => quote! {},
                };
                Some(from_this_crate(quote! { #marked #sig ; }))
            }
            _ => None,
        })
        .collect();
    let name = &args.name;
    let scope: TokenStream2 = args.scope.parse().expect("a module path");
    let trait_name = &port.ident;
    let uses_of_the_port = &args.uses;
    let uses = from_this_crate(quote! { #(#uses_of_the_port),* });
    quote! {
        #[::async_trait::async_trait]
        #port

        #[doc(hidden)]
        #[macro_export]
        macro_rules! #name {
            ($($double:tt)*) => {
                $crate::__forward_finish! {
                    port #trait_name ;
                    scope $crate :: #scope ;
                    uses { #uses }
                    methods { #(#signatures)* }
                    $($double)*
                }
            };
        }
    }
    .into()
}

/// How a double answers the methods it does not write.
enum Mode {
    /// `self.<field>.<method>(...)`.
    Via(Member),
    /// `Err(<expr>)` for each method that returns a `Result`.
    Down(Expr),
}

/// **Hand a double's impl to a port's forwarding macro.** Written on the impl:
/// `#[forward(jojobot_domain::forward_memory, via = inner)]`. The impl states the
/// methods it changes; the field `inner` receives every other call.
#[proc_macro_attribute]
pub fn forward(args: TokenStream, item: TokenStream) -> TokenStream {
    struct Args {
        callback: Path,
        mode: Mode,
        required_only: bool,
        also: Vec<Ident>,
    }
    impl Parse for Args {
        fn parse(input: ParseStream) -> syn::Result<Self> {
            let callback: Path = input.parse()?;
            let mut mode = None;
            let mut required_only = false;
            let mut also = Vec::new();
            while input.parse::<Token![,]>().is_ok() && !input.is_empty() {
                let option: Ident = input.parse()?;
                if option == "via" {
                    input.parse::<Token![=]>()?;
                    mode = Some(Mode::Via(input.parse()?));
                } else if option == "down" {
                    input.parse::<Token![=]>()?;
                    mode = Some(Mode::Down(input.parse()?));
                } else if option == "required_only" {
                    required_only = true;
                } else if option == "also" {
                    let names;
                    syn::parenthesized!(names in input);
                    also = names
                        .parse_terminated(Ident::parse, Token![,])?
                        .into_iter()
                        .collect();
                } else {
                    return Err(syn::Error::new(
                        option.span(),
                        "expected `via`, `down`, `required_only` or `also(<method>, ...)`",
                    ));
                }
            }
            Ok(Self {
                callback,
                mode: mode.ok_or_else(|| input.error("`via = <field>` or `down = <error>`"))?,
                required_only,
                also,
            })
        }
    }
    let Args {
        callback,
        mode,
        required_only,
        also,
    } = parse_macro_input!(args as Args);
    let item = TokenStream2::from(item);
    let mode = match mode {
        Mode::Via(field) => quote! { via #field },
        Mode::Down(error) => quote! { down ( #error ) },
    };
    quote! {
        #callback ! {
            mode #mode ;
            required_only #required_only ;
            also [ #(#also),* ] ;
            #item
        }
    }
    .into()
}

/// **`crate::` in the port's signatures means the port's crate**, and the double
/// is in another one. It becomes `$crate::`, which the macro written beside the
/// port resolves to the port's crate wherever it is called.
fn from_this_crate(tokens: TokenStream2) -> TokenStream2 {
    use proc_macro2::{Group, Punct, Spacing};
    let mut out = TokenStream2::new();
    let mut tokens = tokens.into_iter().peekable();
    while let Some(token) = tokens.next() {
        match token {
            TokenTree::Ident(ident)
                if ident == "crate"
                    && matches!(tokens.peek(), Some(TokenTree::Punct(p)) if p.as_char() == ':') =>
            {
                out.extend([
                    TokenTree::Punct(Punct::new('$', Spacing::Alone)),
                    TokenTree::Ident(ident),
                ]);
            }
            TokenTree::Group(group) => {
                let mut rewritten = Group::new(group.delimiter(), from_this_crate(group.stream()));
                rewritten.set_span(group.span());
                out.extend([TokenTree::Group(rewritten)]);
            }
            other => out.extend([other]),
        }
    }
    out
}

/// What [`__finish`] receives: the port, the scope its names are read in, its
/// methods, how the double answers and the double's impl.
struct Finish {
    port: Ident,
    scope: TokenStream2,
    uses: Vec<TokenStream2>,
    methods: Vec<TraitItemFn>,
    mode: Mode,
    required_only: bool,
    also: Vec<Ident>,
    double: ItemImpl,
}

/// Read a key and check it is the one expected: the grammar is the macro's own.
fn key(input: ParseStream, expected: &str) -> syn::Result<()> {
    let found: Ident = input.parse()?;
    if found == expected {
        Ok(())
    } else {
        Err(syn::Error::new(
            found.span(),
            format!("expected `{expected}`"),
        ))
    }
}

impl Parse for Finish {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        key(input, "port")?;
        let port: Ident = input.parse()?;
        input.parse::<Token![;]>()?;
        key(input, "scope")?;
        // **Read as tokens, not as a path**: it begins with `$crate`, which is
        // not a name a path parser accepts.
        let mut scope = TokenStream2::new();
        while !input.peek(Token![;]) {
            scope.extend([input.parse::<TokenTree>()?]);
        }
        input.parse::<Token![;]>()?;
        key(input, "uses")?;
        let inside;
        braced!(inside in input);
        // **Tokens, not paths**: a path may begin with `$crate`.
        let mut uses = vec![TokenStream2::new()];
        while !inside.is_empty() {
            let token: TokenTree = inside.parse()?;
            match &token {
                TokenTree::Punct(p) if p.as_char() == ',' => uses.push(TokenStream2::new()),
                _ => uses.last_mut().expect("one").extend([token]),
            }
        }
        uses.retain(|path| !path.is_empty());
        key(input, "methods")?;
        let inside;
        braced!(inside in input);
        let mut methods = Vec::new();
        while !inside.is_empty() {
            methods.push(inside.parse::<TraitItemFn>()?);
        }
        key(input, "mode")?;
        let how: Ident = input.parse()?;
        let mode = if how == "via" {
            Mode::Via(input.parse()?)
        } else {
            let error;
            syn::parenthesized!(error in input);
            Mode::Down(error.parse()?)
        };
        input.parse::<Token![;]>()?;
        key(input, "required_only")?;
        let required_only = input.parse::<syn::LitBool>()?.value;
        input.parse::<Token![;]>()?;
        key(input, "also")?;
        let inside;
        syn::bracketed!(inside in input);
        let also = inside
            .parse_terminated(Ident::parse, Token![,])?
            .into_iter()
            .collect();
        input.parse::<Token![;]>()?;
        Ok(Self {
            port,
            scope,
            uses,
            methods,
            mode,
            required_only,
            also,
            double: input.parse()?,
        })
    }
}

/// Whether a method returns a `Result`, which is what a down double can refuse with.
fn returns_a_result(sig: &syn::Signature) -> bool {
    match &sig.output {
        ReturnType::Type(_, ty) => match &**ty {
            Type::Path(path) => path
                .path
                .segments
                .last()
                .is_some_and(|segment| segment.ident == "Result"),
            _ => false,
        },
        ReturnType::Default => false,
    }
}

/// **Not for direct use**: the last step, called by the macro [`port`]
/// writes. It adds the method for each port method the impl leaves out, and puts
/// the impl in a block that reads the port's names.
#[proc_macro]
pub fn __finish(input: TokenStream) -> TokenStream {
    let Finish {
        port,
        scope,
        uses,
        methods,
        mode,
        required_only,
        also,
        mut double,
    } = parse_macro_input!(input as Finish);
    let written: Vec<String> = double
        .items
        .iter()
        .filter_map(|item| match item {
            ImplItem::Fn(method) => Some(method.sig.ident.to_string()),
            _ => None,
        })
        .collect();
    let named_port = match &double.trait_ {
        Some((_, path, _)) => path.segments.last().map(|segment| segment.ident.clone()),
        None => None,
    };
    if named_port.as_ref() != Some(&port) {
        return syn::Error::new(
            Span::call_site(),
            format!("this impl is not an impl of `{port}`, the port its forwarding comes from"),
        )
        .to_compile_error()
        .into();
    }
    for method in &methods {
        let name = method.sig.ident.to_string();
        if written.contains(&name) {
            continue;
        }
        // A double that answers only what the port requires leaves each method
        // with a default body to that body, unless it names the method in `also`.
        let has_default = method
            .attrs
            .iter()
            .any(|attr| attr.path().is_ident("has_default"));
        if required_only && has_default && !also.iter().any(|keep| keep == &name) {
            continue;
        }
        let mut sig = method.sig.clone();
        let body: TokenStream2 = match &mode {
            Mode::Via(field) => {
                let mut calls = Vec::new();
                for (at, input) in sig.inputs.iter_mut().enumerate() {
                    if let FnArg::Typed(typed) = input {
                        // A pattern that is no plain name (`_`) is given one, so it
                        // can be passed on.
                        if !matches!(&*typed.pat, Pat::Ident(_)) {
                            let named = format_ident!("__forwarded_{at}");
                            *typed.pat = syn::parse_quote!(#named);
                        }
                        if let Pat::Ident(named) = &*typed.pat {
                            let ident = &named.ident;
                            calls.push(quote! { #ident });
                        }
                    }
                }
                let ident = &sig.ident;
                let call = quote! { self . #field . #ident ( #(#calls),* ) };
                if sig.asyncness.is_some() {
                    quote! { { #call . await } }
                } else {
                    quote! { { #call } }
                }
            }
            Mode::Down(error) => {
                // A method that returns no `Result` cannot refuse. With a default
                // body it keeps the default, as the port says; without one it is
                // for the double to write.
                if !returns_a_result(&sig) {
                    if has_default {
                        continue;
                    }
                    return syn::Error::new(
                        sig.ident.span(),
                        format!(
                            "a down double cannot answer `{name}`: it returns no `Result` and the \
                             port gives it no default body, so the double writes it"
                        ),
                    )
                    .to_compile_error()
                    .into();
                }
                for input in sig.inputs.iter_mut() {
                    if let FnArg::Typed(typed) = input {
                        *typed.pat = syn::parse_quote!(_);
                    }
                }
                quote! { { Err(#error) } }
            }
        };
        let forwarded: ImplItemFn = syn::parse_quote! { #sig #body };
        double.items.push(ImplItem::Fn(forwarded));
    }
    quote! {
        const _: () = {
            #[allow(unused_imports)]
            use #scope :: * ;
            #[allow(unused_imports)]
            use ::std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
            #[allow(unused_imports)]
            use ::std::sync::Arc;
            #( #[allow(unused_imports)] use #uses ; )*
            #[::async_trait::async_trait]
            #double
        };
    }
    .into()
}
