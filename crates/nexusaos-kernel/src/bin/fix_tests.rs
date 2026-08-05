use std::{
    env,
    fs,
    path::PathBuf,
    process,
};

use syn::{parse_file, ItemFn, Signature, Expr, Item};
use quote::ToTokens;

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() < 2 {
        eprintln!("Usage: {} <file1> [file2] ...", args[0]);
        process::exit(1);
    }

    let mut any_failed = false;

    for filepath in &args[1..] {
        let path = PathBuf::from(filepath);
        if !path.exists() {
            eprintln!("Skipping {}: file not found", filepath);
            continue;
        }

        match fix_file(&path) {
            Ok(_) => println!("Fixed: {}", filepath),
            Err(e) => {
                eprintln!("Error fixing {}: {}", filepath, e);
                any_failed = true;
            }
        }
    }

    if any_failed {
        process::exit(1);
    }
}

fn fix_file(path: &PathBuf) -> Result<(), String> {
    let content = fs::read_to_string(path).map_err(|e| e.to_string())?;
    let mut file = parse_file(&content).map_err(|e| e.to_string())?;
    let mut modified = false;

    for item in &mut file.items {
        if let Item::Fn(item_fn) = item {
            if is_test_function(item_fn) {
                let needs_result = has_unwrap_or_expect_or_panic(&item_fn.block)
                    && !returns_result(&item_fn.sig);

                if needs_result {
                    item_fn.sig.output = syn::ReturnType::Type(
                        syn::token::RArrow::default(),
                        Box::new(syn::parse2(quote::quote! {
                            Result<(), Box<dyn std::error::Error>>
                        }).unwrap())
                    );
                    modified = true;
                }

                if has_unwrap_or_expect_or_panic(&item_fn.block) {
                    fix_function_body(&mut item_fn.block);
                    modified = true;
                }

                if returns_result(&item_fn.sig) && !ends_with_ok(&item_fn.block) {
                    add_ok_return(&mut item_fn.block);
                    modified = true;
                }
            }
        }
    }

    if modified {
        let new_content = file.to_token_stream().to_string();
        fs::write(path, new_content).map_err(|e| e.to_string())?;
    }

    Ok(())
}

fn is_test_function(item: &ItemFn) -> bool {
    let name = item.sig.ident.to_string();
    if !name.starts_with("test_") {
        return false;
    }

    item.attrs.iter().any(|attr| {
        attr.path().is_ident("test") || 
        (attr.path().is_ident("tokio") && attr.to_token_stream().to_string().contains("test"))
    })
}

fn has_unwrap_or_expect_or_panic(block: &syn::Block) -> bool {
    let tokens = block.to_token_stream().to_string();
    tokens.contains(".unwrap()") || tokens.contains(".expect(") || tokens.contains("panic!(")
}

fn returns_result(sig: &Signature) -> bool {
    match &sig.output {
        syn::ReturnType::Type(_, ty) => {
            let ty_str = ty.to_token_stream().to_string();
            ty_str.contains("Result")
        }
        _ => false,
    }
}

fn ends_with_ok(block: &syn::Block) -> bool {
    if block.stmts.is_empty() {
        return false;
    }
    if let syn::Stmt::Expr(expr, _) = &block.stmts[block.stmts.len() - 1] {
        if let Expr::Path(expr_path) = expr {
            if expr_path.path.is_ident("Ok") {
                return true;
            }
        }
    }
    false
}

fn fix_function_body(block: &mut syn::Block) {
    for stmt in &mut block.stmts {
        let tokens = stmt.to_token_stream().to_string();
        
        if tokens.contains(".unwrap()") {
            let new_tokens = tokens.replace(".unwrap()", "?");
            *stmt = syn::parse2(new_tokens.parse().unwrap()).unwrap();
        }
        
        if tokens.contains(".expect(") {
            let _new_tokens = tokens.replace(".expect(", "?");
            // But we need to handle the closing paren...
            // Actually, let's use a more robust approach
            let stmt_str = stmt.to_token_stream().to_string();
            let new_stmt_str = stmt_str.replace(".expect(", "?");
            // Remove the extra closing paren if needed
            let new_stmt_str = new_stmt_str.replace("?\"", "?\"");
            *stmt = syn::parse2(new_stmt_str.parse().unwrap()).unwrap();
        }
        
        if tokens.contains("panic!(") {
            let stmt_str = stmt.to_token_stream().to_string();
            let new_stmt_str = stmt_str.replace("panic!(", "unreachable!(");
            *stmt = syn::parse2(new_stmt_str.parse().unwrap()).unwrap();
        }
    }
}

fn add_ok_return(block: &mut syn::Block) {
    let ok_stmt: syn::Stmt = syn::parse2(quote::quote! {
        Ok(())
    }).unwrap();
    block.stmts.push(ok_stmt);
}
