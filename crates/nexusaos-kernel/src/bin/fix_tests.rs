use std::{env, fs, path::PathBuf, process};

use quote::ToTokens;
use syn::{parse_file, Expr, Item, ItemFn, Signature};

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() < 2 {
        eprintln!("Usage: {} <file1> [file2] ...", args[0]);
        process::exit(1);
    }

    for filepath in &args[1..] {
        let path = PathBuf::from(filepath);
        println!("Processing: {}", filepath);
        if !path.exists() {
            eprintln!("  Skipping: file not found");
            continue;
        }

        match fix_file(&path) {
            Ok(_) => println!("  Fixed: {}", filepath),
            Err(e) => eprintln!("  Error: {}", e),
        }
    }
}

fn fix_file(path: &PathBuf) -> Result<(), String> {
    let content = fs::read_to_string(path).map_err(|e| e.to_string())?;
    println!("  File length: {}", content.len());
    let file = parse_file(&content).map_err(|e| e.to_string())?;
    println!("  Parsed {} top-level items", file.items.len());
    let mut modified = false;
    let mut test_count = 0;

    for item in &file.items {
        test_count += process_item(item, &mut modified);
    }

    println!("  Total test functions found: {}", test_count);
    println!("  Modified: {}", modified);

    if modified {
        let new_content = file.to_token_stream().to_string();
        fs::write(path, new_content).map_err(|e| e.to_string())?;
    }

    Ok(())
}

fn process_item(item: &Item, modified: &mut bool) -> usize {
    match item {
        Item::Fn(item_fn) => {
            if is_test_function(item_fn) {
                println!("  [TEST] {}", item_fn.sig.ident);
                println!("    returns_result: {}", returns_result(&item_fn.sig));
                println!("    has_unwrap: {}", has_unwrap_or_expect_or_panic(&item_fn.block));
                println!("    ends_with_ok: {}", ends_with_ok(&item_fn.block));

                let needs_result =
                    has_unwrap_or_expect_or_panic(&item_fn.block) && !returns_result(&item_fn.sig);

                if needs_result {
                    println!("    -> Converting to Result return type");
                    *modified = true;
                }

                if has_unwrap_or_expect_or_panic(&item_fn.block) && returns_result(&item_fn.sig) {
                    println!("    -> Fixing function body");
                    *modified = true;
                }

                if returns_result(&item_fn.sig) && !ends_with_ok(&item_fn.block) {
                    println!("    -> Adding Ok(())");
                    *modified = true;
                }

                return 1;
            }
            println!("  [FN] {}", item_fn.sig.ident);
            0
        }
        Item::Mod(item_mod) => {
            println!(
                "  [MOD] {} (items: {})",
                item_mod.ident,
                item_mod.content.as_ref().map(|c| c.1.len()).unwrap_or(0)
            );
            let mut count = 0;
            if let Some((_, ref items)) = item_mod.content {
                for inner_item in items {
                    count += process_item(inner_item, modified);
                }
            }
            count
        }
        _ => {
            println!(
                "  [OTHER] {}",
                item.to_token_stream().to_string().split_whitespace().next().unwrap_or("?")
            );
            0
        }
    }
}

fn is_test_function(item: &ItemFn) -> bool {
    let name = item.sig.ident.to_string();
    if !name.starts_with("test_") {
        return false;
    }
    item.attrs.iter().any(|attr| {
        attr.path().is_ident("test")
            || (attr.path().is_ident("tokio")
                && attr.to_token_stream().to_string().contains("test"))
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
    if let syn::Stmt::Expr(Expr::Path(expr_path), _) = &block.stmts[block.stmts.len() - 1] {
        if expr_path.path.is_ident("Ok") {
            return true;
        }
    }
    false
}
