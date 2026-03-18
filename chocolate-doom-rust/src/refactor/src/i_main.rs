//
// Copyright(C) 1993-1996 Id Software, Inc.
// Copyright(C) 2005-2014 Simon Howard
//
// This program is free software; you can redistribute it and/or
// modify it under the terms of the GNU General Public License
// as published by the Free Software Foundation; either version 2
// of the License, or (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.
//
// DESCRIPTION:
//	Main program, simply calls D_DoomMain high level loop.
//
use std::ffi::{CString, CStr};
use std::os::raw::{c_char, c_void, c_int};
use std::env;

lazy_mut!{
    static mut cmdArgs : Vec::<String> = Vec::new();
}

#[no_mangle]
pub unsafe extern "C" fn rust_main(argc : c_int , argv : &[*mut c_char]) -> c_int {
    
    //Saving cmdargs
    let arglen = argc as usize;
    for x in 0..arglen {
        let var = CStr::from_ptr(argv[x]);
        let printable = var.to_string_lossy().to_string();
        cmdArgs.push(printable);
    }
    
    //M_FindResponseFile()

    //Start Doom

    return 0;
}


#[no_mangle]
pub unsafe extern "C" fn M_CheckParm(check : *const i8) -> c_int {
    return M_CheckParmWithArgs(check, 0);
}

//
// M_CheckParm
// Checks for the given parameter
// in the program's command line arguments.
// Returns the argument number (1 to argc-1)
// or 0 if not present
//
#[no_mangle]
pub unsafe extern "C" fn M_CheckParmWithArgs(check: *const i8, num_args: c_int) -> c_int {
    let v = CStr::from_ptr(check);
    let checkval = v.to_string_lossy().to_string();
    let argc = num_args as usize;

    for i in 1..cmdArgs.len() - argc {
        if cmdArgs[i] == checkval {
            let result = i as c_int;
            return result;
        }
    }

    return 0;
}

#[no_mangle]
pub unsafe extern "C" fn M_ParmExists(check : *const i8) -> c_int {
   
    if M_CheckParm(check) != 0 {
        return 1; //Value of doom true
    }else {
        return 0; //Value of doom false
    }
}

#[no_mangle]
pub unsafe extern "C" fn M_GetArg(index: c_int) -> *const i8 {
    let value = CString::new(cmdArgs[0].clone()).unwrap();
    return value.as_c_str().as_ptr();
}

#[no_mangle]
pub unsafe extern "C" fn M_ArgCount() -> c_int {
    let count = cmdArgs.len() as c_int;
    return count;
}

#[no_mangle]
pub unsafe extern "C" fn M_GetExecutableName() -> *const i8 {
    return M_GetArg(0);
}