use crate::commands::command::*;
use crate::priority::{get_priority, CommandPriority};
use crate::set::set::{error_log, get_last};
use crate::process::process::ProcessManager;
use crate::process::sleep;
use crate::process::thread::ThreadControlBlock;
use crate::process::add_task::{add_command_to_thread,add_thread_to_process};
use crate::root::SessionContext;
use crate::signal::semaphore_new;

use std::sync::atomic::{AtomicUsize, Ordering};

use crate::commands::arg::{command_match, split, Commands};

static NEXT_TID: AtomicUsize = AtomicUsize::new(1);
static NEXT_PID: AtomicUsize = AtomicUsize::new(1);


pub fn handle_command(args: Vec<String>) -> (Commands,usize,usize,CommandPriority) {
    let commands = Commands::new(args.clone());
    let command = commands.command.clone();
    let priority = get_priority(command.as_str());

    let tid = NEXT_TID.fetch_add(1, Ordering::SeqCst);

    let pid = NEXT_PID.fetch_add(1, Ordering::SeqCst);

    (commands,pid,tid,priority)
}


pub fn run(args: Vec<String>,session_context: &mut SessionContext){
    let (commands,pid,tid,priority) = handle_command(args);
    let semaphore = semaphore_new();
    let mut tcb = ThreadControlBlock::new();
    let mut pcb = ProcessManager::new();


    let (command,_option,arg) = split(commands.clone());

    add_command_to_thread(tid, command.clone(), priority, &mut tcb);
    add_thread_to_process(pid, command.clone(), tcb.clone(), semaphore, &mut pcb);
    
    let priority_tid = tcb.get_highest_priority_thread().unwrap();
    tcb.start_thread(priority_tid);
    pcb.start_process(pid);

    
    match command.as_str(){
        "ps" => pcb.ps(),
        "kill" => match arg.is_empty(){
            true => {},
            _flase => pcb.kill(arg[0].parse::<usize>().unwrap())
        },
        "sleep" => sleep(&mut tcb, commands.arg[0].parse::<usize>().unwrap()),
        _ =>{
            // start process and thread
            if session_context.user_state.root.check_permission(){
                // Execute root commands
                // Handle commands differently when user is in root mode
                if let Ok(res) = command_match(commands, session_context){
                    let status = res.0.clone();
                    let result = res.1.clone();
                    if status==0{
                        println!("[{}] Done\n{}",tid,result);
                        pcb.kill(pid);
                        tcb.stop_thread(priority_tid);
                    }else{
                        error_log(res.1.clone());
                        println!("{}",res.1);
                    }
                }
            } else if !session_context.user_state.root.check_permission() && !session_context.root.allowed_commands.contains(&commands.command) {
                // Execute normal commands
                // Handle commands normally when user is not in root mode
                if let Ok(res) = command_match(commands, session_context) {
                    let status = res.0.clone();
                    let result = res.1.clone();
                    if status==0{
                        println!("[{}] Done\n{}",tid,result);
                        pcb.kill(pid);
                        tcb.stop_thread(priority_tid);
                    }else{
                        error_log(res.1.clone());
                        println!("{}",res.1);
                    }
                }
            }else{
                eprintln!("Permission not support")
            }
        }
    }
    
}


use rustyline::Editor;
use rustyline::error::ReadlineError;

pub fn init_shell(session_context: &mut SessionContext){
    // init an Editor
    let mut rl = Editor::<()>::new();
    loop {
        let readline: Result<String, ReadlineError> = rl.readline(&print_prompt(session_context));
        match readline {
            Ok(line) => {
                if line.trim().is_empty() {
                    continue;
                }
                // add Key::UP and Key::DOWN to find history
                rl.add_history_entry(line.clone());
                // add line in lazy HISTORY
                history_push(line.clone());
                
                if line.parse::<usize>().is_ok() {
                    let (_i, res) = get_last(line.parse::<usize>().unwrap());
                    match res {
                        Some(command) => {
                            let args: Vec<String> = command.split_whitespace().map(|s| s.to_string()).collect();
                            run(args, session_context);
                        }
                        None => {
                            continue;
                        }
                    }
                } else {
                    let args: Box<Vec<String>> = Box::new(line.split_whitespace().map(|s| s.to_string()).collect());
                    if args.contains(&"&&".to_string()) {
                        and(*args, session_context)
                    } else if args.contains(&"|".to_string()) {
                        let s = pipe(*args).unwrap();
                        println!("{}", s.1);
                    } else if args.contains(&"&".to_string()) {
                        priority_run(*args, session_context)
                    } else {
                        run(*args, session_context);
                    }
                    
                }
            }
            Err(ReadlineError::Interrupted) => {
                println!("CTRL-C");
                break;
            }
            Err(ReadlineError::Eof) => {
                println!("CTRL-D");
                break;
            }
            Err(err) => {
                println!("Error: {:?}", err);
                break;
            }
        }
    }
}

// root
fn print_prompt(session_context: &mut SessionContext) -> String{
    let mut whoami = session_context.get_username();
    if session_context.user_state.root.check_permission(){
        whoami="root".to_string()
    }
    let pwd = pwd().unwrap().1;
    let input = format!("\x1B[32;1m{}\x1B[0m:\x1B[34m{}>>\x1B[0m ",whoami,pwd); // Assuming whoami() returns the current user
    input
}