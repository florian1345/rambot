use proc_macro::TokenStream;

use proc_macro2::Span;

use quote::ToTokens;

use syn::{
    parse_macro_input,
    Attribute,
    AttributeArgs,
    AttrStyle,
    Block,
    FnArg,
    Ident,
    ItemFn,
    Lit,
    Meta,
    NestedMeta,
    Path,
    PathSegment,
    PathArguments,
    PatType,
    Stmt,
    Type
};
use syn::punctuated::Punctuated;
use syn::token::{Brace, Bracket, Pound};

struct CommandData {
    name: String,
    description: Option<String>,
    usage: Option<String>,
    rest: bool
}

fn path_to_string(p: &Path) -> Option<String> {
    if p.segments.len() > 1 {
        return None;
    }

    let segment = p.segments.first().unwrap();
    
    if !segment.arguments.is_empty() {
        return None;
    }

    Some(segment.ident.to_string())
}

fn apply_arg(cmd_data: &mut CommandData, arg: &NestedMeta) -> bool {
    match arg {
        NestedMeta::Meta(Meta::Path(p)) => {
            if let Some(name) = path_to_string(p) {
                match name.as_str() {
                    "rest" => cmd_data.rest = true,
                    _ => return false
                }
    
                true
            }
            else {
                false
            }
        },
        NestedMeta::Meta(Meta::NameValue(nv)) => {
            let value = match &nv.lit {
                Lit::Str(s) => s.value(),
                _ => return false
            };

            if let Some(name) = path_to_string(&nv.path) {
                match name.as_str() {
                    "name" => cmd_data.name = value,
                    "description" => cmd_data.description = Some(value),
                    "usage" => cmd_data.usage = Some(value),
                    _ => return false
                }
    
                true
            }
            else {
                false
            }
        },
        _ => false
    }
}

fn load_data(cmd_data: &mut CommandData, attr: AttributeArgs)
        -> Result<(), String> {
    for arg in attr {
        let ok = apply_arg(cmd_data, &arg);

        if !ok {
            return Err(format!("invalid command arguent: {}",
                arg.to_token_stream()));
        }
    }

    Ok(())
}

fn add_attribute(item: &mut ItemFn, name: &str, value: &str) {
    let tokens = quote::quote! { (#value) };
    let mut segments = Punctuated::new();
    segments.push(PathSegment {
        ident: Ident::new(name, Span::call_site()),
        arguments: PathArguments::None
    });

    item.attrs.push(Attribute {
        pound_token: Pound::default(),
        style: AttrStyle::Outer,
        bracket_token: Bracket::default(),
        path: Path {
            leading_colon: None,
            segments
        },
        tokens
    })
}

// We choose a high-entropy name to avoid collisions.
const ARGS_NAME: &str = "args_ae833f7d85d462b66742f3c4b1a704b6";

#[derive(Copy, Clone, Eq, PartialEq)]
enum ArgType {
    Plain,
    Option,
    Vec
}

fn arg_type(arg: &PatType) -> ArgType {
    match arg.ty.as_ref() {
        Type::Path(p) => {
            let seg = &p.path.segments;

            if seg.len() != 1 {
                return ArgType::Plain;
            }

            let seg = seg.first().unwrap();
            let ident = seg.ident.to_string();

            match ident.as_str() {
                "Option" => ArgType::Option,
                "Vec" => ArgType::Vec,
                _ => ArgType::Plain
            }
        },
        _ => ArgType::Plain
    }
}

fn rambot_command_do(attr: Vec<NestedMeta>, mut item: ItemFn)
        -> Result<TokenStream, String> {
    let mut cmd_data = CommandData {
        name: item.sig.ident.to_string(),
        description: None,
        usage: None,
        rest: false
    };

    load_data(&mut cmd_data, attr)?;

    // Serenity attributes

    add_attribute(&mut item, "command", &cmd_data.name);

    if let Some(description) = &cmd_data.description {
        add_attribute(&mut item, "description", description.as_str());
    }

    if let Some(usage) = &cmd_data.usage {
        add_attribute(&mut item, "usage", usage.as_str());
    }

    item.attrs.push(syn::parse_quote! {
        #[only_in(guilds)]
    });

    // Parse and check arguments

    if item.sig.inputs.len() < 2 {
        return Err("missing context and/or message arguments".to_owned());
    }

    let mut to_parse = Vec::new();

    while item.sig.inputs.len() > 2 {
        let arg = item.sig.inputs.pop().unwrap().into_value();

        match arg {
            FnArg::Receiver(_) =>
                return Err("self argument on command function".to_owned()),
            FnArg::Typed(arg) => to_parse.push(arg)
        }
    }

    to_parse.reverse();

    let mut encountered_option = false;
    let mut encountered_vec = false;

    for arg in &to_parse {
        if encountered_vec {
            return Err("cannot have argument after a vector".to_owned());
        }

        match arg_type(arg) {
            ArgType::Plain => {
                if encountered_option {
                    return Err("cannot have non-optional argument after \
                        optional argument".to_owned())
                }
            },
            ArgType::Option => encountered_option = true,
            ArgType::Vec => encountered_vec = true
        }
    }

    if cmd_data.rest && (encountered_option || encountered_vec) {
        return Err("cannot have option or vec parsed as rest".to_owned());
    }

    // Construct new signature

    let args_ident = Ident::new(ARGS_NAME, Span::call_site());
    let args_param: FnArg = syn::parse_quote! {
        mut #args_ident : serenity::framework::standard::Args
    };

    item.sig.inputs.push(args_param);

    // Construct new body

    let mut new_body = Block {
        brace_token: Brace::default(),
        stmts: Vec::new()
    };
    let last_idx = to_parse.len().wrapping_sub(1);

    for (idx, arg) in to_parse.into_iter().enumerate() {
        let stmt: Stmt = if idx == last_idx && cmd_data.rest {
            syn::parse_quote! {
                let #arg = #args_ident.rest().parse()?;
            }
        }
        else {
            match arg_type(&arg) {
                ArgType::Plain => syn::parse_quote! {
                    let #arg = #args_ident.single_quoted()?;
                },
                ArgType::Option => syn::parse_quote! {
                    let #arg = if #args_ident.is_empty() {
                        None
                    }
                    else {
                        Some(#args_ident.single_quoted()?)
                    };
                },
                ArgType::Vec => syn::parse_quote! {
                    let #arg = {
                        let mut v = Vec::new();

                        while !#args_ident.is_empty() {
                            v.push(#args_ident.single_quoted()?);
                        }

                        v
                    };
                }
            }
        };

        new_body.stmts.push(stmt);
    }

    if !cmd_data.rest {
        new_body.stmts.push(syn::parse_quote! {
            #[derive(Debug)]
            struct ExpectedEndError;
        });
    
        new_body.stmts.push(syn::parse_quote! {
            impl std::fmt::Display for ExpectedEndError {
                fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                    write!(f, "Expected end, but received more arguments.")
                }
            }
        });
    
        new_body.stmts.push(syn::parse_quote! {
            impl std::error::Error for ExpectedEndError { }
        });
    
        new_body.stmts.push(syn::parse_quote! {
            if !#args_ident.is_empty() {
                return Err(ExpectedEndError.into());
            }
        });
    }

    new_body.stmts.append(&mut item.block.as_mut().stmts);
    item.block = Box::new(new_body);

    Ok(item.to_token_stream().into())
}

/// An adapted procedural macro for Rambot commands that invokes the Serenity
/// command framework to enable its functionality. The main additional feature
/// is automatic parsing of function arguments.
///
/// # Attribute parameters
///
/// The macro has four different available options, which are provided as
/// key-value parameters of the form `#[rambot_command(key = "value")]`. or as
/// standalone flags of the form `#[rambot_command(flag)]`. Multiple parameters
/// are separated by commas, as normal.
///
/// * `name`: Has as value a string which defines the name of the command. If
/// this is not set, the command name will be equal to the function identifier.
/// * `description`: Has as value a string which offers a description of the
/// command to be displayed in the `help` command.
/// * `usage`: Has as value a string which constitutes an example usage sans
/// command name.
/// * `rest`: A flag which indicates that the last function argument should be
/// parsed from the rest string after parsing all other arguments. Cannot be
/// used in conjunction with option or vector arguments
///
/// # Argument parsing
///
/// This macro automatically generates code for all parameters beyond the
/// `&Context` and `&Message` parameters to parse their values. Any argument
/// of some type that implements [FromStr] can be used here. Use [Option] for
/// optional arguments and [Vec] for a trailing argument list. Note that all
/// non-optional values must come first, then all optional ones, and then at
/// most one [Vec].
///
/// # Example usage
///
/// ```ignore
/// // In this example, we define two arguments, the second of which is parsed
/// // as the rest of the command string.
///
/// #[rambot_command(
///     description = "Adds an effect to the layer with the given name.",
///     usage = "<layer> <effect>",
///     rest
/// )]
/// async fn add(ctx: &Context, msg: &Message, layer: String, effect: String)
///         -> CommandResult {
///     [...]
/// }
/// ```
#[proc_macro_attribute]
pub fn rambot_command(attr: TokenStream, item: TokenStream) -> TokenStream {
    let attr = parse_macro_input!(attr as AttributeArgs);
    let item = parse_macro_input!(item as ItemFn);

    match rambot_command_do(attr, item) {
        Ok(s) => s,
        Err(message) => {
            let span = Span::call_site();
            quote::quote_spanned!(span => compile_error! { #message }).into()
        }
    }
}
