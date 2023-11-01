use common::code::{Asm, Code};
use common::error::NovaError;
use common::gen::Gen;
use common::nodes::{Ast, Atom, Expr};
use common::tokens::TType;

#[derive(Clone)]
pub struct Compiler {
    pub bindings: common::table::Table<String>,
    pub global: common::table::Table<String>,
    pub variables: common::table::Table<String>,
    pub upvalues: common::table::Table<String>,
    pub native_functions: common::table::Table<String>,
    pub output: Vec<u8>,
    pub filepath: String,
    pub entry: usize,
    pub asm: Vec<Asm>,
    pub gen: Gen,
    pub breaks: Vec<usize>,
}

pub fn new() -> Compiler {
    Compiler {
        native_functions: common::table::new(),
        variables: common::table::new(),
        output: Vec::new(),
        filepath: String::new(),
        upvalues: common::table::new(),
        global: common::table::new(),
        entry: 0,
        bindings: common::table::new(),
        asm: vec![],
        gen: common::gen::new(),
        breaks: vec![],
    }
}

impl Compiler {
    pub fn clear(&mut self) {
        self.output.clear()
    }

    pub fn get_entry(&self) -> usize {
        self.entry
    }

    #[inline(always)]
    pub fn compile_program(
        &mut self,
        input: Ast,
        filepath: String,
        alloc: bool,
        global: bool,
        function: bool,
    ) -> Result<Vec<u8>, NovaError> {
        self.filepath = filepath;
        for statements in input.program.iter() {
            match statements {
                common::nodes::Statement::Pass => {}
                common::nodes::Statement::Let(_, identifier, expr) => {
                    self.compile_expr(expr.clone())?;
                    if let Some(index) = self.variables.get_index(identifier.to_string()) {
                        self.asm.push(Asm::STORE(index as u32))
                    } else {
                        self.variables.insert(identifier.to_string());
                        let index = self.variables.len() - 1;
                        self.asm.push(Asm::STORE(index as u32))
                    }
                }
                common::nodes::Statement::Function(_, identifier, parameters, input) => {
                    self.global.insert(identifier.to_string());
                    let mut function_compile = self.clone();
                    function_compile.variables.clear();
                    function_compile.asm.clear();
                    for args in parameters.iter() {
                        function_compile
                            .variables
                            .insert(args.identifier.to_string());
                    }
                    let functionjump = function_compile.gen.generate();
                    self.asm.push(Asm::FUNCTION(functionjump));

                    let function_body = Ast {
                        program: input.clone(),
                    };
                    let _ = function_compile.compile_program(
                        function_body,
                        self.filepath.clone(),
                        true,
                        false,
                        true,
                    )?;
                    self.asm.push(Asm::OFFSET(
                        parameters.len() as u32,
                        (function_compile.variables.len() - parameters.len()) as u32,
                    ));
                    self.gen = function_compile.gen;
                    function_compile.asm.pop();
                    self.asm.extend_from_slice(&function_compile.asm);
                    self.asm.push(Asm::LABEL(functionjump));
                    let index = self.global.len() - 1;
                    self.asm.push(Asm::STOREGLOBAL(index as u32));
                }

                common::nodes::Statement::Struct(_, identifier, fields) => {
                    self.global.insert(identifier.to_string());
                    let structjump = self.gen.generate();
                    self.asm.push(Asm::FUNCTION(structjump));
                    self.asm
                        .push(Asm::OFFSET((fields.len() - 1) as u32, 0 as u32));
                    self.asm.push(Asm::STRING(identifier.clone()));
                    self.asm.push(Asm::LIST(fields.len()));
                    self.asm.push(Asm::RET(true));
                    self.asm.push(Asm::LABEL(structjump));
                    let index = self.global.len() - 1;
                    self.asm.push(Asm::STOREGLOBAL(index as u32));
                }

                common::nodes::Statement::Return(ttype, expr, _, _) => {
                    self.compile_expr(expr.clone())?;
                    if ttype != &TType::Void {
                        self.asm.push(Asm::RET(true))
                    } else {
                        self.asm.push(Asm::RET(false))
                    }
                }
                common::nodes::Statement::Expression(_, expr) => self.compile_expr(expr.clone())?,
                common::nodes::Statement::If(_, test, body, alternative) => {
                    let (bodyjump, alterjump) = (self.gen.generate(), self.gen.generate());
                    self.compile_expr(test.clone())?;
                    self.asm.push(Asm::JUMPIFFALSE(bodyjump));
                    let body_ast = Ast {
                        program: body.clone(),
                    };
                    self.compile_program(body_ast, self.filepath.clone(), false, false, false)?;
                    self.asm.pop();

                    if let Some(alternative) = alternative {
                        self.asm.push(Asm::JMP(alterjump));
                        self.asm.push(Asm::LABEL(bodyjump));
                        let alt = Ast {
                            program: alternative.clone(),
                        };
                        self.compile_program(alt, self.filepath.clone(), false, false, false)?;
                        self.asm.pop();
                        self.asm.push(Asm::LABEL(alterjump));
                    } else {
                        self.asm.push(Asm::LABEL(bodyjump));
                    }
                }

                common::nodes::Statement::While(test, body) => {
                    let top = self.gen.generate();
                    let end = self.gen.generate();
                    self.breaks.push(end);
                    self.asm.push(Asm::LABEL(top));
                    self.compile_expr(test.clone())?;
                    self.asm.push(Asm::JUMPIFFALSE(end));
                    let whilebody = Ast {
                        program: body.clone(),
                    };
                    self.compile_program(whilebody, self.filepath.clone(), false, false, false)?;
                    self.asm.pop();
                    self.asm.push(Asm::BJMP(top));
                    self.asm.push(Asm::LABEL(end));
                    self.breaks.pop();
                }
                common::nodes::Statement::For(init, test, inc, body) => {
                    let top = self.gen.generate();
                    let end = self.gen.generate();
                    self.breaks.push(end);
                    self.compile_expr(init.clone())?;
                    self.asm.push(Asm::LABEL(top));
                    self.compile_expr(test.clone())?;
                    self.asm.push(Asm::JUMPIFFALSE(end));
                    let whilebody = Ast {
                        program: body.clone(),
                    };
                    self.compile_program(whilebody, self.filepath.clone(), false, false, false)?;
                    self.asm.pop();
                    self.compile_expr(inc.clone())?;
                    self.asm.push(Asm::BJMP(top));
                    self.asm.push(Asm::LABEL(end));
                    self.breaks.pop();
                }
                common::nodes::Statement::Break => {
                    if let Some(target) = self.breaks.last() {
                        self.asm.push(Asm::JMP(*target));
                    } else {
                        todo!()
                    }
                }
                common::nodes::Statement::Continue => todo!(),
                common::nodes::Statement::Block(body) => {
                    let body = Ast {
                        program: body.clone(),
                    };
                    self.compile_program(body, self.filepath.clone(), false, false, false)?;
                    self.asm.pop();
                }
            }
        }

        if function {
        } else if alloc {
            //self.output = self.load_package(self.output.clone());
            self.asm
                .insert(0, Asm::ALLOCLOCALS(self.variables.len() as u32));
        }

        if global {
            //self.output = self.load_global(self.output.clone());
            self.asm
                .insert(0, Asm::ALLOCGLOBBALS(self.global.len() as u32));
        }

        // self.output.push(Code::RET);
        // self.output.push(0);
        self.asm.push(Asm::RET(false));
        Ok(self.output.to_owned())
    }

    pub fn getref_expr(&mut self, expr: Expr) -> Result<(), NovaError> {
        match expr {
            Expr::None => {
                //self.output.push(Code::NONE)
            }
            Expr::ListConstructor(_, _) => todo!(),
            Expr::Field(_, _, index, from) => {
                self.asm.push(Asm::INTEGER(index as i64));
                self.getref_expr(*from)?;
                self.asm.push(Asm::PIN);
            }
            Expr::Indexed(_, _, index, from) => {
                match &*index {
                    Expr::Literal(_, atom) => match atom {
                        Atom::Id(id) => {
                            if let Some(index) = self.variables.get_index(id.to_string()) {
                                self.asm.push(Asm::STACKREF(index as u32));
                            }
                        }
                        Atom::Integer(int) => self.asm.push(Asm::INTEGER(*int)),
                        _ => {
                            panic!()
                        }
                    },
                    _ => {}
                }
                self.getref_expr(*from)?;
                self.asm.push(Asm::PIN);
            }
            Expr::Call(_, _, _, _) => todo!(),
            Expr::Unary(_, _, _) => todo!(),
            Expr::Binop(_, _, _, _) => todo!(),
            Expr::Literal(_, atom) => {
                self.getref_atom(atom)?;
            }
            Expr::Closure(_, _, _, _) => todo!(),
        }
        Ok(())
    }

    pub fn getref_atom(&mut self, atom: Atom) -> Result<(), NovaError> {
        match atom {
            Atom::Bool(bool) => {
                if bool {
                    self.asm.push(Asm::BOOL(true));
                } else {
                    self.asm.push(Asm::BOOL(false));
                }
            }
            Atom::Id(identifier) => {
                if let Some(index) = self.variables.get_index(identifier.to_string()) {
                    self.asm.push(Asm::STACKREF(index as u32));
                } else {
                    dbg!(identifier);
                    todo!()
                }
            }
            Atom::Float(float) => {
                self.asm.push(Asm::FLOAT(float));
            }
            Atom::String(str) => {
                self.asm.push(Asm::STRING(str.clone()));
            }
            Atom::Integer(int) => {
                self.asm.push(Asm::INTEGER(int));
            }
            Atom::Call(caller, list) => {
                for expr in list.iter() {
                    self.compile_expr(expr.clone())?
                }
                match caller.as_str() {
                    "print" => self.asm.push(Asm::PRINT),
                    "free" => self.asm.push(Asm::FREE),
                    "clone" => self.asm.push(Asm::CLONE),
                    identifier => {
                        if let Some(index) = self.variables.get_index(identifier.to_string()) {
                            self.asm.push(Asm::GET(index as u32));
                            self.asm.push(Asm::CALL);
                        } else if let Some(index) = self.global.get_index(identifier.to_string()) {
                            self.asm.push(Asm::DCALL(index as u32));
                        } else {
                            dbg!(identifier);
                            todo!()
                        }
                    }
                }
            }
        }
        Ok(())
    }

    pub fn compile_expr(&mut self, expr: Expr) -> Result<(), NovaError> {
        match expr {
            Expr::None => {
                //    Ok(self.output.push(Code::NONE))
                Ok(())
            }
            Expr::ListConstructor(_, list) => {
                for x in list.iter().cloned() {
                    self.compile_expr(x)?;
                }
                self.asm.push(Asm::LIST(list.len()));
                Ok(())
            }
            Expr::Field(_, _, index, field) => {
                self.asm.push(Asm::INTEGER(index as i64));
                self.compile_expr(*field)?;
                self.asm.push(Asm::LIN);
                Ok(())
            }
            Expr::Indexed(_, _, index, from) => {
                self.compile_expr(*index)?;
                self.compile_expr(*from)?;
                self.asm.push(Asm::LIN);
                Ok(())
            }
            Expr::Call(_, _, from, arg) => {
                for e in arg.iter().cloned() {
                    self.compile_expr(e)?;
                }
                self.getref_expr(*from)?;
                self.asm.push(Asm::CALL);
                Ok(())
            }
            Expr::Unary(_, unary, expr) => match unary {
                common::tokens::Unary::Positive => todo!(),
                common::tokens::Unary::Negitive => {
                    self.compile_expr(*expr)?;
                    self.asm.push(Asm::NEG);
                    Ok(())
                }
                common::tokens::Unary::Not => {
                    self.compile_expr(*expr)?;
                    Ok(self.asm.push(Asm::NOT))
                }
            },
            Expr::Binop(ttype, operator, lhs, rhs) => {
                match operator {
                    common::tokens::Operator::RightArrow => todo!(),
                    common::tokens::Operator::GreaterThan => {
                        self.compile_expr(*lhs.clone())?;
                        self.compile_expr(*rhs)?;
                        if lhs.get_type() == TType::Int {
                            self.asm.push(Asm::IGTR);
                        } else if lhs.get_type() == TType::Float {
                            self.asm.push(Asm::FGTR);
                        } else {
                            dbg!(&ttype);
                        }
                    }
                    common::tokens::Operator::LessThan => {
                        self.compile_expr(*lhs.clone())?;
                        self.compile_expr(*rhs)?;
                        if lhs.get_type() == TType::Int {
                            self.asm.push(Asm::ILSS);
                        } else if lhs.get_type() == TType::Float {
                            self.asm.push(Asm::FLSS);
                        } else {
                            dbg!(&ttype);
                        }
                    }
                    common::tokens::Operator::Assignment => {
                        self.compile_expr(*rhs.clone())?;
                        self.getref_expr(*lhs.clone())?;

                        self.asm.push(Asm::ASSIGN)
                    }
                    common::tokens::Operator::Addition => {
                        self.compile_expr(*lhs.clone())?;
                        self.compile_expr(*rhs)?;
                        if lhs.get_type() == TType::Int {
                            self.asm.push(Asm::IADD);
                        } else if lhs.get_type() == TType::Float {
                            self.asm.push(Asm::FADD);
                        } else {
                            dbg!(&ttype);
                        }
                    }
                    common::tokens::Operator::Subtraction => {
                        self.compile_expr(*lhs.clone())?;
                        self.compile_expr(*rhs)?;
                        if lhs.get_type() == TType::Int {
                            self.asm.push(Asm::ISUB);
                        } else if lhs.get_type() == TType::Float {
                            self.asm.push(Asm::FSUB);
                        } else {
                            dbg!(&ttype);
                        }
                    }
                    common::tokens::Operator::Division => {
                        self.compile_expr(*lhs.clone())?;
                        self.compile_expr(*rhs)?;
                        if lhs.get_type() == TType::Int {
                            self.asm.push(Asm::IDIV);
                        } else if lhs.get_type() == TType::Float {
                            self.asm.push(Asm::FDIV);
                        } else {
                            dbg!(&ttype);
                        }
                    }
                    common::tokens::Operator::Multiplication => {
                        self.compile_expr(*lhs.clone())?;
                        self.compile_expr(*rhs)?;
                        if lhs.get_type() == TType::Int {
                            self.asm.push(Asm::IMUL);
                        } else if lhs.get_type() == TType::Float {
                            self.asm.push(Asm::FMUL);
                        } else {
                            dbg!(&ttype);
                        }
                    }
                    common::tokens::Operator::Equality => {
                        self.compile_expr(*lhs)?;
                        self.compile_expr(*rhs)?;
                        self.asm.push(Asm::EQUALS);
                    }
                    common::tokens::Operator::Access => todo!(),
                    common::tokens::Operator::ListAccess => todo!(),
                    common::tokens::Operator::Call => todo!(),
                    common::tokens::Operator::Modulo => {
                        self.compile_expr(*lhs)?;
                        self.compile_expr(*rhs)?;
                        self.asm.push(Asm::IMODULO);
                    }
                    common::tokens::Operator::NotEqual => {
                        self.compile_expr(*lhs)?;
                        self.compile_expr(*rhs)?;
                        self.asm.push(Asm::EQUALS);
                        self.asm.push(Asm::NOT);
                    }
                    common::tokens::Operator::Not => {
                        self.compile_expr(*lhs)?;
                        self.compile_expr(*rhs)?;
                        self.asm.push(Asm::NOT);
                    }
                    common::tokens::Operator::DoubleColon => todo!(),
                    common::tokens::Operator::Colon => todo!(),
                    common::tokens::Operator::GtrOrEqu => {
                        let sc = self.gen.generate();

                        // if lhs is true, return its value
                        // else return the other value
                        self.compile_expr(*lhs.clone())?;
                        self.compile_expr(*rhs.clone())?;
                        if lhs.get_type() == TType::Int {
                            self.asm.push(Asm::IGTR);
                        } else if lhs.get_type() == TType::Float {
                            self.asm.push(Asm::FGTR);
                        } else {
                            dbg!(&ttype);
                        }
                        self.asm.push(Asm::DUP);
                        self.asm.push(Asm::NOT);
                        self.asm.push(Asm::JUMPIFFALSE(sc));
                        self.asm.push(Asm::POP);
                        self.compile_expr(*lhs)?;
                        self.compile_expr(*rhs)?;
                        self.asm.push(Asm::EQUALS);
                        self.asm.push(Asm::LABEL(sc))
                    }
                    common::tokens::Operator::LssOrEqu => {
                        let sc = self.gen.generate();

                        // if lhs is true, return its value
                        // else return the other value
                        self.compile_expr(*lhs.clone())?;
                        self.compile_expr(*rhs.clone())?;
                        if lhs.get_type() == TType::Int {
                            self.asm.push(Asm::ILSS);
                        } else if lhs.get_type() == TType::Float {
                            self.asm.push(Asm::FLSS);
                        } else {
                            dbg!(&ttype);
                        }
                        self.asm.push(Asm::DUP);
                        self.asm.push(Asm::NOT);
                        self.asm.push(Asm::JUMPIFFALSE(sc));
                        self.asm.push(Asm::POP);
                        self.compile_expr(*lhs)?;
                        self.compile_expr(*rhs)?;
                        self.asm.push(Asm::EQUALS);
                        self.asm.push(Asm::LABEL(sc))
                    }
                    common::tokens::Operator::And => {
                        let sc = self.gen.generate();

                        // if lhs is false, return its value
                        // else return other value
                        self.compile_expr(*lhs)?;
                        self.asm.push(Asm::DUP);
                        self.asm.push(Asm::JUMPIFFALSE(sc));
                        self.asm.push(Asm::POP);
                        self.compile_expr(*rhs)?;
                        self.asm.push(Asm::LABEL(sc))
                    }
                    common::tokens::Operator::Or => {
                        let sc = self.gen.generate();

                        // if lhs is true, return its value
                        // else return the other value
                        self.compile_expr(*lhs)?;
                        self.asm.push(Asm::DUP);
                        self.asm.push(Asm::NOT);
                        self.asm.push(Asm::JUMPIFFALSE(sc));
                        self.asm.push(Asm::POP);
                        self.compile_expr(*rhs)?;
                        self.asm.push(Asm::LABEL(sc))
                    }
                    common::tokens::Operator::AdditionAssignment => {
                        self.compile_expr(*rhs.clone())?;
                        self.compile_expr(*lhs.clone())?;
                        if lhs.get_type() == TType::Int {
                            self.asm.push(Asm::IADD);
                        } else if lhs.get_type() == TType::Float {
                            self.asm.push(Asm::FADD);
                        } else {
                            dbg!(&ttype);
                        }
                        self.getref_expr(*lhs.clone())?;

                        self.asm.push(Asm::ASSIGN)
                    }
                    common::tokens::Operator::SubtractionAssignment => {
                        self.compile_expr(*rhs.clone())?;
                        self.compile_expr(*lhs.clone())?;
                        if lhs.get_type() == TType::Int {
                            self.asm.push(Asm::ISUB);
                        } else if lhs.get_type() == TType::Float {
                            self.asm.push(Asm::FSUB);
                        } else {
                            dbg!(&ttype);
                        }
                        self.getref_expr(*lhs.clone())?;

                        self.asm.push(Asm::ASSIGN)
                    }
                }
                Ok(())
            }
            Expr::Literal(_, atom) => self.compile_atom(atom),
            Expr::Closure(_, parameters, input, captured) => {
                let mut function_compile = self.clone();
                function_compile.variables.clear();
                function_compile.asm.clear();
                for args in parameters.iter() {
                    function_compile
                        .variables
                        .insert(args.identifier.to_string());
                }
                for args in captured.iter() {
                    function_compile.variables.insert(args.to_string());
                }
                for x in captured.iter().cloned() {
                    if let Some(index) = self.variables.get_index(x.to_string()) {
                        self.asm.push(Asm::GET(index as u32));
                    } else {
                        panic!()
                    }
                }
                self.asm.push(Asm::LIST(captured.len()));
                let closurejump = function_compile.gen.generate();
                self.asm.push(Asm::CLOSURE(closurejump));

                let function_body = Ast {
                    program: input.clone(),
                };
                let _ = function_compile.compile_program(
                    function_body,
                    self.filepath.clone(),
                    true,
                    false,
                    true,
                )?;
                self.asm.push(Asm::OFFSET(
                    (parameters.len() + captured.len()) as u32,
                    ((parameters.len() + captured.len()) - function_compile.variables.len()) as u32,
                ));
                self.gen = function_compile.gen;
                function_compile.asm.pop();
                self.asm.extend_from_slice(&function_compile.asm);
                self.asm.push(Asm::LABEL(closurejump));
                Ok(())
            }
        }
    }

    pub fn compile_atom(&mut self, atom: Atom) -> Result<(), NovaError> {
        match atom {
            Atom::Bool(bool) => {
                if bool {
                    self.asm.push(Asm::BOOL(true));
                } else {
                    self.asm.push(Asm::BOOL(false));
                }
            }
            Atom::Id(identifier) => {
                if let Some(index) = self.variables.get_index(identifier.to_string()) {
                    self.asm.push(Asm::GET(index as u32));
                } else if let Some(index) = self.global.get_index(identifier.to_string()) {
                    self.asm.push(Asm::GETGLOBAL(index as u32));
                }
            }
            Atom::Float(float) => {
                self.asm.push(Asm::FLOAT(float));
            }
            Atom::String(str) => {
                self.asm.push(Asm::STRING(str.clone()));
            }
            Atom::Integer(int) => {
                self.asm.push(Asm::INTEGER(int));
            }
            Atom::Call(caller, list) => {
                for expr in list.iter() {
                    self.compile_expr(expr.clone())?
                }
                match caller.as_str() {
                    "super::free" => self.asm.push(Asm::FREE),
                    "super::clone" => self.asm.push(Asm::CLONE),
                    "super::print" => {
                        self.asm.push(Asm::PRINT);
                        self.output.push(Code::PRINT)
                    }
                    identifier => {
                        if let Some(index) = self.native_functions.get_index(identifier.to_string())
                        {
                            self.asm.push(Asm::NATIVE(index))
                        } else {
                            if let Some(index) = self.variables.get_index(identifier.to_string()) {
                                self.asm.push(Asm::GET(index as u32));
                                self.asm.push(Asm::CALL);
                            } else if let Some(index) =
                                self.global.get_index(identifier.to_string())
                            {
                                self.asm.push(Asm::DCALL(index as u32));
                            } else {
                                dbg!(identifier);
                                todo!()
                            }
                        }
                    }
                }
            }
        }
        Ok(())
    }
}
