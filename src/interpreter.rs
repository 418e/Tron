use crate::environment::Environment;
use crate::expr::{CallableImpl, LiteralValue, NativeFunctionImpl, TronFunctionImpl};
use crate::parser::*;
use crate::resolver::*;
use crate::scanner::Token;
use crate::scanner::*;
use crate::settings;
use crate::stmt::Stmt;
use colored::Colorize;
use std::collections::HashMap;
use std::fs;
use std::io;
use std::process::exit;
use std::process::Command;
use std::rc::Rc;
pub struct Interpreter {
    pub specials: HashMap<String, LiteralValue>,
    pub environment: Environment,
}
impl Interpreter {
    pub fn new() -> Self {
        Self {
            specials: HashMap::new(),
            environment: Environment::new(HashMap::new()),
        }
    }
    pub fn resolve(&mut self, locals: HashMap<usize, usize>) {
        self.environment.resolve(locals);
    }
    pub fn with_env(env: Environment) -> Self {
        Self {
            specials: HashMap::new(),
            environment: env,
        }
    }

    pub fn interpret(&mut self, stmts: Vec<&Stmt>) -> Result<(), String> {
        for stmt in stmts {
            match stmt {
                Stmt::Expression { expression } => {
                    expression.evaluate(self.environment.clone())?;
                }
                Stmt::Print { expression } => {
                    let value = expression.evaluate(self.environment.clone())?;
                    let pointer_setting = settings("pointer");

                    if pointer_setting == "default" {
                        println!(" ➤ {}", value.to_string().green());
                    } else {
                        println!(" {} {}", pointer_setting, value.to_string().green());
                    }
                }
                Stmt::Input { expression } => {
                    let value = expression.evaluate(self.environment.clone())?;
                    if settings("pointer") == "default" {
                        println!(" ➤ {}", value.to_string());
                    } else {
                        println!(" {} {}", settings("pointer"), value.to_string());
                    }
                    let mut input = String::new();
                    io::stdin().read_line(&mut input).unwrap();
                }
                Stmt::Errors { expression } => {
                    let value = expression.evaluate(self.environment.clone())?;
                    if settings("pointer") == "default" {
                        println!(" ➤ {}", value.to_string().red().to_string());
                    } else {
                        println!(
                            " {} {}",
                            settings("pointer"),
                            value.to_string().red().to_string()
                        );
                    }
                    exit(1)
                }
                Stmt::Exits {} => exit(1),
                Stmt::Import { expression } => {
                    let value = expression.evaluate(self.environment.clone())?;
                    let val = value.to_string();

                    // Extract path removing the enclosing quotes
                    fn rem_first_and_last(value: &str) -> &str {
                        &value[1..value.len() - 1]
                    }

                    let run_file = |path: &str| -> Result<(), String> {
                        let absolute_path = if path.starts_with('/') {
                            path.to_string()
                        } else {
                            let current_dir = std::env::current_dir().map_err(|e| e.to_string())?;
                            current_dir
                                .join(path)
                                .to_str()
                                .ok_or("Invalid path")?
                                .to_string()
                        };

                        let contents =
                            fs::read_to_string(&absolute_path).map_err(|e| e.to_string())?;
                        run_string(&contents)
                    };

                    fn run_string(contents: &str) -> Result<(), String> {
                        let mut interpreter = Interpreter::new();
                        run(&mut interpreter, contents)
                    }
                    fn run(interpreter: &mut Interpreter, contents: &str) -> Result<(), String> {
                        let scanner = Scanner::new(contents);
                        let tokens = scanner.scan_tokens().map_err(|e| e.to_string())?;
                        let mut parser = Parser::new(tokens);
                        let stmts: Vec<Stmt> = parser.parse().map_err(|e| e.to_string())?;
                        let stmts_refs: Vec<&Stmt> = stmts.iter().collect();
                        let resolver = Resolver::new();
                        let locals = resolver.resolve(&stmts_refs)?;
                        interpreter.resolve(locals);
                        interpreter.interpret(stmts_refs)?;
                        Ok(())
                    }

                    match run_file(rem_first_and_last(&val)) {
                        Ok(_) => {}
                        Err(msg) => {
                            println!("Error 108:\n{}", msg);
                            exit(1);
                        }
                    }
                }
                Stmt::Var { name, initializer } => {
                    let value = initializer.evaluate(self.environment.clone())?;
                    self.environment.define(name.lexeme.clone(), value);
                }
                Stmt::Block { statements } => {
                    let new_environment = self.environment.enclose();
                    let old_environment = self.environment.clone();
                    self.environment = new_environment;
                    let block_result =
                        self.interpret((*statements).iter().map(|b| b.as_ref()).collect());
                    self.environment = old_environment;
                    block_result?;
                }
                Stmt::IfStmt {
                    predicate,
                    then,
                    elif_branches,
                    els,
                } => {
                    let truth_value = predicate.evaluate(self.environment.clone())?;
                    if truth_value.is_truthy() == LiteralValue::True {
                        self.interpret(vec![then.as_ref()])?;
                    } else {
                        let mut executed_branch = false;
                        for (elif_predicate, elif_stmt) in elif_branches {
                            let elif_truth_value =
                                elif_predicate.evaluate(self.environment.clone())?;
                            if elif_truth_value.is_truthy() == LiteralValue::True {
                                self.interpret(vec![elif_stmt.as_ref()])?;
                                executed_branch = true;
                                break;
                            }
                        }
                        if !executed_branch {
                            if let Some(els_stmt) = els {
                                self.interpret(vec![els_stmt.as_ref()])?;
                            }
                        }
                    }
                }
                Stmt::TryStmt { tri, catch } => {
                    let result = self.interpret(vec![tri.as_ref()]);
                    match result {
                        Ok(_) => {
                            self.interpret(vec![tri.as_ref()])?;
                        }
                        Err(_) => {
                            self.interpret(vec![catch.as_ref()])?;
                        }
                    }
                }
                Stmt::WhileStmt { condition, body } => {
                    while condition.evaluate(self.environment.clone())?.is_truthy()
                        == LiteralValue::True
                    {
                        match self.interpret(vec![body.as_ref()]) {
                            Ok(_) => {}
                            Err(e) if e == "break" => break, // Check for a "break" error to exit the loop
                            Err(e) => return Err(e),         // Propagate other errors
                        }
                    }
                }
                Stmt::BenchStmt { body } => {
                    let start_time = std::time::SystemTime::now();
                    let statements = vec![body.as_ref()];
                    self.interpret(statements)?;
                    let end_time = std::time::SystemTime::now().duration_since(start_time);
                    if end_time.clone().unwrap().as_micros() < 10000 {
                        println!("{:?}µs", end_time.unwrap().as_micros());
                    } else if end_time.clone().unwrap().as_micros() > 10000 {
                        println!("{:?}ms", end_time.unwrap().as_millis());
                    } else if end_time.clone().unwrap().as_millis() > 10000 {
                        println!("{:?}s", end_time.unwrap().as_secs_f32());
                    } else {
                        println!("{:?}µs", end_time.unwrap().as_micros());
                    }
                }
                Stmt::Function {
                    name,
                    params: _,
                    body: _,
                } => {
                    let callable = self.make_function(stmt);
                    let fun = LiteralValue::Callable(CallableImpl::TronFunction(callable));
                    self.environment.define(name.lexeme.clone(), fun);
                }
                Stmt::CmdFunction { name, cmd } => {
                    let cmd = cmd.clone();
                    let local_fn = move |_args: &Vec<LiteralValue>| {
                        let cmd = cmd.clone();
                        let parts = cmd.split(" ").collect::<Vec<&str>>();
                        let mut command = Command::new(parts[0].replace("\"", ""));
                        for part in parts[1..].iter() {
                            command.arg(part.replace("\"", ""));
                        }
                        let output = command.output().expect("Failed to run command");
                        return LiteralValue::StringValue(
                            std::str::from_utf8(output.stdout.as_slice())
                                .unwrap()
                                .to_string(),
                        );
                    };
                    let fun_val =
                        LiteralValue::Callable(CallableImpl::NativeFunction(NativeFunctionImpl {
                            name: name.lexeme.clone(),
                            arity: 0,
                            fun: Rc::new(local_fn),
                        }));
                    self.environment.define(name.lexeme.clone(), fun_val);
                }
                Stmt::ReturnStmt { keyword: _, value } => {
                    let eval_val;
                    if let Some(value) = value {
                        eval_val = value.evaluate(self.environment.clone())?;
                    } else {
                        eval_val = LiteralValue::Nil;
                    }
                    self.specials.insert("return".to_string(), eval_val);
                }
                Stmt::BreakStmt { .. } => {
                    return Err("break".to_string());
                }
            };
        }
        Ok(())
    }
    fn make_function(&self, fn_stmt: &Stmt) -> TronFunctionImpl {
        if let Stmt::Function { name, params, body } = fn_stmt {
            let arity = params.len();
            let params: Vec<Token> = params.iter().map(|t| (*t).clone()).collect();
            let body: Vec<Box<Stmt>> = body.iter().map(|b| (*b).clone()).collect();
            let name_clone = name.lexeme.clone();
            let parent_env = self.environment.clone();
            let callable_impl = TronFunctionImpl {
                name: name_clone,
                arity,
                parent_env,
                params,
                body,
            };
            callable_impl
        } else {
            panic!("Tried to make a function from a non-function statement");
        }
    }
}
