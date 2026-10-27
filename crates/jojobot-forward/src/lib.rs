//! **A test double names the methods it changes; the forwarding of the rest is
//! generated from the port.**
//!
//! Three pieces, in the order they run:
//!
//! 1. [`forwardable`] sits on a port trait. It leaves the trait as it is and
//!    writes a `macro_rules!` beside it that carries the trait's method list.
//! 2. [`forward`] sits on the `impl` of a double. It hands the impl to that macro.
//! 3. [`__finish`] receives the method list and the impl together, and adds a
//!    forwarding method for each port method the impl does not write.
//!
//! The callback through a `macro_rules!` is how the impl sees a trait that is
//! declared in another crate: a proc macro cannot read another item, but a macro
//! the trait wrote can carry its method list to one.
//!
//! **What a forwarded method does**: `self.<field>.<method>(<args>)`, awaited when
//! the port method is async. The field is named by `via`. A method the impl
//! writes is left as it was written.

use proc_macro::TokenStream;
use proc_macro2::{Span, TokenStream as TokenStream2};
use quote::{format_ident, quote};
use syn::parse::{Parse, ParseStream};
use syn::{
    Ident, ImplItem, ImplItemFn, ItemImpl, ItemTrait, LitStr, Member, Path, Token, TraitItem,
    TraitItemFn, braced, parse_macro_input,
};

/// The arguments of [`forwardable`]: `macro = <name>, scope = "<module>"`.
struct PortArgs {
    name: Ident,
    scope: String,
}

impl Parse for PortArgs {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let mut name = None;
        let mut scope = None;
        while !input.is_empty() {
            let key: Ident = input.parse()?;
            input.parse::<Token![=]>()?;
            if key == "callback" {
                name = Some(input.parse::<Ident>()?);
            } else if key == "scope" {
                scope = Some(input.parse::<LitStr>()?.value());
            } else {
                return Err(syn::Error::new(
                    key.span(),
                    "expected `callback` or `scope`",
                ));
            }
            if !input.is_empty() {
                input.parse::<Token![,]>()?;
            }
        }
        Ok(Self {
            name: name.ok_or_else(|| input.error("`callback = <name>` is required"))?,
            scope: scope.ok_or_else(|| input.error("`scope = \"<module>\"` is required"))?,
        })
    }
}

/// **Put a port's method list where a double can reach it.** Written on the trait:
/// `#[forwardable(callback = forward_memory, scope = "memory")]`. The trait is
/// emitted unchanged. Beside it comes an exported macro of the given name, which
/// is what [`forward`] calls; `scope` is the module of the crate that declares
/// the trait, whose public names the forwarded signatures are read in.
#[proc_macro_attribute]
pub fn forwardable(args: TokenStream, item: TokenStream) -> TokenStream {
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
    quote! {
        #port

        #[doc(hidden)]
        #[macro_export]
        macro_rules! #name {
            ($($double:tt)*) => {
                $crate::__forward_finish! {
                    port #trait_name ;
                    scope $crate :: #scope ;
                    methods { #(#signatures)* }
                    $($double)*
                }
            };
        }
    }
    .into()
}

/// **Hand a double's impl to a port's forwarding macro.** Written on the impl:
/// `#[forward(jojobot_domain::forward_memory, via = inner)]`. The impl states the
/// methods it changes; the field `inner` receives every other call.
#[proc_macro_attribute]
pub fn forward(args: TokenStream, item: TokenStream) -> TokenStream {
    struct Args {
        callback: Path,
        via: Member,
        required_only: bool,
        also: Vec<Ident>,
    }
    impl Parse for Args {
        fn parse(input: ParseStream) -> syn::Result<Self> {
            let callback: Path = input.parse()?;
            input.parse::<Token![,]>()?;
            let key: Ident = input.parse()?;
            if key != "via" {
                return Err(syn::Error::new(key.span(), "expected `via = <field>`"));
            }
            input.parse::<Token![=]>()?;
            let via: Member = input.parse()?;
            let mut required_only = false;
            let mut also = Vec::new();
            while input.parse::<Token![,]>().is_ok() && !input.is_empty() {
                let option: Ident = input.parse()?;
                if option == "required_only" {
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
                        "expected `required_only` or `also(<method>, ...)`",
                    ));
                }
            }
            Ok(Self {
                callback,
                via,
                required_only,
                also,
            })
        }
    }
    let Args {
        callback,
        via,
        required_only,
        also,
    } = parse_macro_input!(args as Args);
    let item = TokenStream2::from(item);
    quote! {
        #callback ! { via #via ; required_only #required_only ; also [ #(#also),* ] ; #item }
    }
    .into()
}

/// **`crate::` in the port's signatures means the port's crate**, and the double
/// is in another one. It becomes `$crate::`, which the macro written beside the
/// port resolves to the port's crate wherever it is called.
fn from_this_crate(tokens: TokenStream2) -> TokenStream2 {
    use proc_macro2::{Group, Punct, Spacing, TokenTree};
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
/// methods and the double's impl.
struct Finish {
    port: Ident,
    scope: TokenStream2,
    methods: Vec<TraitItemFn>,
    via: Member,
    required_only: bool,
    also: Vec<Ident>,
    double: ItemImpl,
}

impl Parse for Finish {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let key: Ident = input.parse()?;
        assert_eq!(key, "port");
        let port: Ident = input.parse()?;
        input.parse::<Token![;]>()?;
        let key: Ident = input.parse()?;
        assert_eq!(key, "scope");
        // **Read as tokens, not as a path**: it begins with `$crate`, which is
        // not a name a path parser accepts.
        let mut scope = TokenStream2::new();
        while !input.peek(Token![;]) {
            scope.extend([input.parse::<proc_macro2::TokenTree>()?]);
        }
        input.parse::<Token![;]>()?;
        let key: Ident = input.parse()?;
        assert_eq!(key, "methods");
        let inside;
        braced!(inside in input);
        let mut methods = Vec::new();
        while !inside.is_empty() {
            methods.push(inside.parse::<TraitItemFn>()?);
        }
        let key: Ident = input.parse()?;
        assert_eq!(key, "via");
        let via: Member = input.parse()?;
        input.parse::<Token![;]>()?;
        let key: Ident = input.parse()?;
        assert_eq!(key, "required_only");
        let required_only = input.parse::<syn::LitBool>()?.value;
        input.parse::<Token![;]>()?;
        let key: Ident = input.parse()?;
        assert_eq!(key, "also");
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
            methods,
            via,
            required_only,
            also,
            double: input.parse()?,
        })
    }
}

/// **Not for direct use**: the last step, called by the macro [`forwardable`]
/// writes. It adds the forwarding method for each port method the impl leaves
/// out, and puts the impl in a block that reads the port's names.
#[proc_macro]
pub fn __finish(input: TokenStream) -> TokenStream {
    let Finish {
        port,
        scope,
        methods,
        via,
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
        // A double that forwards only what the port requires leaves each method
        // with a default body to that body, unless it names the method in `also`.
        let has_default = method
            .attrs
            .iter()
            .any(|attr| attr.path().is_ident("has_default"));
        if required_only && has_default && !also.iter().any(|keep| keep == &name) {
            continue;
        }
        let sig = method.sig.clone();
        let mut calls = Vec::new();
        let mut inputs = sig.inputs.clone();
        for (at, input) in inputs.iter_mut().enumerate() {
            if let syn::FnArg::Typed(typed) = input {
                // A pattern that is no plain name (`_`) is given one, so it can be passed on.
                if !matches!(&*typed.pat, syn::Pat::Ident(_)) {
                    let named = format_ident!("__forwarded_{at}");
                    *typed.pat = syn::parse_quote!(#named);
                }
                if let syn::Pat::Ident(name) = &*typed.pat {
                    let ident = &name.ident;
                    calls.push(quote! { #ident });
                }
            }
        }
        let mut sig = sig;
        sig.inputs = inputs;
        let ident = &sig.ident;
        let call = quote! { self . #via . #ident ( #(#calls),* ) };
        let body: TokenStream2 = if sig.asyncness.is_some() {
            quote! { { #call . await } }
        } else {
            quote! { { #call } }
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
            #[allow(unused_imports)]
            use ::jiff::civil::Date;
            #[::async_trait::async_trait]
            #double
        };
    }
    .into()
}
