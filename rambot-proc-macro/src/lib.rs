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
    Pat,
    Path,
    PathArguments,
    PathSegment,
    PatType,
    ReturnType,
    Stmt,
    Type
};
use syn::punctuated::Punctuated;
use syn::token::{Brace, Bracket, Pound};

struct CommandData {
    name: String,
    description: Option<String>,
    usage: Option<String>,
    owner_only: bool,
    rest: bool,
    confirm: bool
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
                    "confirm" => cmd_data.confirm = true,
                    "owners_only" => cmd_data.owner_only = true,
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

fn add_attribute(item: &mut ItemFn, name: &str, value: Option<&str>) {
    let mut segments = Punctuated::new();

    segments.push(PathSegment {
        ident: Ident::new(name, Span::call_site()),
        arguments: PathArguments::None
    });

    let tokens = if let Some(value) = value {
        quote::quote! { (#value) }
    }
    else {
        quote::quote! { }
    };

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

fn get_name(arg: &PatType) -> Result<Ident, String> {
    match arg.pat.as_ref() {
        Pat::Ident(ident) => Ok(ident.ident.clone()),
        Pat::Wild(_) => Ok(Ident::new("_", Span::call_site())),
        _ => Err("unsupported argument pattern".to_owned()),
    }
}

fn process_return_type(t: &mut Type) -> bool {
    // TODO check that the type is actually correct

    match t {
        Type::Path(p) => {
            let seg = &mut p.path.segments;

            if seg.len() != 1 {
                return false;
            }

            let seg = seg.first_mut().unwrap();
            seg.arguments = PathArguments::None;
            true
        },
        _ => false
    }
}

fn get_ident(arg: &FnArg) -> Result<Ident, String> {
    if let FnArg::Typed(arg) = arg {
        if let Pat::Ident(ident) = arg.pat.as_ref() {
            Ok(ident.ident.clone())
        }
        else {
            Err("context or message not identified".to_owned())
        }
    }
    else {
        Err("self argument on command function".to_owned())
    }
}

fn rambot_command_do(attr: Vec<NestedMeta>, mut item: ItemFn)
        -> Result<TokenStream, String> {
    let mut cmd_data = CommandData {
        name: item.sig.ident.to_string(),
        description: None,
        usage: None,
        owner_only: false,
        rest: false,
        confirm: false
    };

    load_data(&mut cmd_data, attr)?;

    // Serenity attributes

    add_attribute(&mut item, "command", Some(&cmd_data.name));

    if let Some(description) = &cmd_data.description {
        add_attribute(&mut item, "description", Some(description.as_str()));
    }

    if let Some(usage) = &cmd_data.usage {
        add_attribute(&mut item, "usage", Some(usage.as_str()));
    }

    if cmd_data.owner_only {
        add_attribute(&mut item, "owners_only", None);
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

    let ctx_ident = get_ident(&item.sig.inputs[0])?;
    let msg_ident = get_ident(&item.sig.inputs[1])?;

    // Construct new signature

    let args_ident = Ident::new(ARGS_NAME, Span::call_site());
    let args_param: FnArg = syn::parse_quote! {
        mut #args_ident : serenity::framework::standard::Args
    };

    item.sig.inputs.push(args_param);

    if let ReturnType::Type(_, output_type) = &mut item.sig.output {
        if !process_return_type(output_type.as_mut()) {
            return Err("invalid return type".to_owned());
        }
    }
    else {
        return Err("missing return type".to_owned());
    }

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
                ArgType::Plain => {
                    let name = get_name(&arg)?.to_string();
                    let reply =
                        format!("Missing mandatory argument `{}`.", name);

                    syn::parse_quote! {
                        let #arg = if #args_ident.is_empty() {
                            #msg_ident.reply(#ctx_ident, #reply).await?;
                            return Ok(())
                        }
                        else {
                            #args_ident.single_quoted()?
                        };
                    }
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
            if !#args_ident.is_empty() {
                #msg_ident.reply(#ctx_ident,
                    "Expected end, but received more arguments.").await?;
                return Ok(());
            }
        });
    }

    let old_body = item.block.as_ref();

    new_body.stmts.push(syn::parse_quote! {
        let result:
            serenity::framework::standard::CommandResult<Option<String>> =
                (|| async #old_body)().await;
    });

    new_body.stmts.push(syn::parse_quote! {
        let result = result?;
    });

    if cmd_data.confirm {
        new_body.stmts.push(syn::parse_quote! {
            match result {
                Some(message) => {
                    #msg_ident.reply(#ctx_ident, message).await?;
                    Ok(())
                },
                None => {
                    if #msg_ident.kind !=
                            serenity::model::channel::MessageType::Unknown {
                        #msg_ident.react(#ctx_ident, '\u{1f44c}').await?;
                    }

                    Ok(())
                }
            }
        });
    }
    else {
        new_body.stmts.push(syn::parse_quote! {
            match result {
                Some(message) => {
                    #msg_ident.reply(#ctx_ident, message).await?;
                    Ok(())
                },
                None => Ok(())
            }
        });
    }

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
///   this is not set, the command name will be equal to the function
///   identifier.
/// * `description`: Has as value a string which offers a description of the
///   command to be displayed in the `help` command.
/// * `usage`: Has as value a string which constitutes an example usage sans
///   command name.
/// * `rest`: A flag which indicates that the last function argument should be
///   parsed from the rest string after parsing all other arguments. Cannot be
///   used in conjunction with option or vector arguments
/// * `confirm`: A flag which indicates whether successful execution of the
///   command should be indicated by an `:ok_hand:` reaction on the message.
/// * `owners_only`: If this flag is present, the attribute of the same name
///   will be added to the command. This restricts the usage of the annotated
///   command to users who are owners of the bot.
///
/// # Argument parsing
///
/// This macro automatically generates code for all parameters beyond the
/// `&Context` and `&Message` parameters to parse their values. Any argument
/// of some type that implements [FromStr](std::str::FromStr) can be used here.
/// Use [Option] for optional arguments and [Vec] for a trailing argument list.
/// Note that all non-optional values must come first, then all optional ones,
/// and then at most one [Vec].
///
/// # Error handling
///
/// The macro transforms functions of the signature
/// `fn(...) -> CommandResult<Option<String>>` to ones of the signature
/// `fn(...) -> CommandResult`. If present, the return value may return an
/// error message, which is automatically responded to the user. Otherwise, if
/// `confirm` is set, a confirmative reaction is added.
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
///         -> CommandResult<Option<String>> {
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
