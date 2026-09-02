//! Z3 最小动态 FFI(kernel32 LoadLibrary/GetProcAddress 运行时加载,无需导入库与外部 crate)。
//!
//! DLL 查找顺序:
//! 1. 环境变量 `AETHER_Z3_PATH`(指向 libz3.dll 完整路径);
//! 2. pip `z3-solver` 用户安装路径(本机默认);
//! 3. 系统 PATH 中的 `libz3.dll` / `z3.dll`。
//!
//! 仅绑定 M3 需要的子集:整数/布尔/数组理论 + solver + model 字符串。

#![allow(non_camel_case_types)]
#![allow(non_snake_case)]
#![cfg(windows)]

use std::ffi::{c_char, c_void, CString};

pub type Z3_bool = i32;
pub type Z3_lbool = i32;
pub type Z3_string = *const c_char;

#[repr(C)]
pub struct Z3_config {
    _priv: [u8; 0],
}
#[repr(C)]
pub struct Z3_context {
    _priv: [u8; 0],
}
#[repr(C)]
pub struct Z3_ast {
    _priv: [u8; 0],
}
#[repr(C)]
pub struct Z3_sort {
    _priv: [u8; 0],
}
#[repr(C)]
pub struct Z3_symbol {
    _priv: [u8; 0],
}
#[repr(C)]
pub struct Z3_solver {
    _priv: [u8; 0],
}
#[repr(C)]
pub struct Z3_model {
    _priv: [u8; 0],
}

#[link(name = "kernel32")]
extern "system" {
    fn LoadLibraryA(lpFileName: *const c_char) -> *mut c_void;
    fn GetProcAddress(hModule: *mut c_void, lpProcName: *const c_char) -> *mut c_void;
}

unsafe fn load_symbol<T: Copy>(module: *mut c_void, name: &str) -> Result<T, String> {
    let cname = CString::new(name).map_err(|_| format!("invalid symbol name {}", name))?;
    let ptr = unsafe { GetProcAddress(module, cname.as_ptr()) };
    if ptr.is_null() {
        return Err(format!("missing symbol {} in Z3 library", name));
    }
    // fn 指针与数据指针在 x64 上同尺寸
    Ok(unsafe { std::mem::transmute_copy::<*mut c_void, T>(&ptr) })
}

macro_rules! z3fns {
    ($(fn $name:ident($($arg:ident : $ty:ty),*) -> $ret:ty;)*) => {
        pub struct Z3Api {
            _module: *mut c_void,
            $(pub $name: unsafe extern "system" fn($($ty),*) -> $ret,)*
        }

        impl Z3Api {
            pub unsafe fn load() -> Result<Self, String> {
                let path = find_z3_dll()?;
                let cpath = CString::new(path.to_string_lossy().as_bytes())
                    .map_err(|_| format!("invalid path {:?}", path))?;
                let module = unsafe { LoadLibraryA(cpath.as_ptr()) };
                if module.is_null() {
                    return Err(format!("LoadLibrary failed for {}", path.display()));
                }
                Ok(Z3Api {
                    _module: module,
                    $($name: unsafe { load_symbol(module, stringify!($name)) }?,)*
                })
            }
        }
    };
}

z3fns! {
    fn Z3_mk_config() -> *mut Z3_config;
    fn Z3_set_param_value(c: *mut Z3_config, param: Z3_string, value: Z3_string) -> ();
    fn Z3_mk_context(c: *mut Z3_config) -> *mut Z3_context;
    fn Z3_del_context(c: *mut Z3_context) -> ();
    fn Z3_mk_solver(c: *mut Z3_context) -> *mut Z3_solver;
    fn Z3_solver_inc_ref(c: *mut Z3_context, s: *mut Z3_solver) -> ();
    fn Z3_solver_dec_ref(c: *mut Z3_context, s: *mut Z3_solver) -> ();
    fn Z3_solver_push(c: *mut Z3_context, s: *mut Z3_solver) -> ();
    fn Z3_solver_pop(c: *mut Z3_context, s: *mut Z3_solver, n: u32) -> ();
    fn Z3_solver_assert(c: *mut Z3_context, s: *mut Z3_solver, a: *mut Z3_ast) -> ();
    fn Z3_solver_check(c: *mut Z3_context, s: *mut Z3_solver) -> Z3_lbool;
    fn Z3_solver_get_model(c: *mut Z3_context, s: *mut Z3_solver) -> *mut Z3_model;
    fn Z3_model_to_string(c: *mut Z3_context, m: *mut Z3_model) -> Z3_string;
    fn Z3_mk_int_sort(c: *mut Z3_context) -> *mut Z3_sort;
    fn Z3_mk_bool_sort(c: *mut Z3_context) -> *mut Z3_sort;
    fn Z3_mk_array_sort(c: *mut Z3_context, domain: *mut Z3_sort, range: *mut Z3_sort) -> *mut Z3_sort;
    fn Z3_mk_string_symbol(c: *mut Z3_context, s: Z3_string) -> *mut Z3_symbol;
    fn Z3_mk_const(c: *mut Z3_context, s: *mut Z3_symbol, ty: *mut Z3_sort) -> *mut Z3_ast;
    fn Z3_mk_int(c: *mut Z3_context, v: i32, ty: *mut Z3_sort) -> *mut Z3_ast;
    fn Z3_mk_numeral(c: *mut Z3_context, numeral: Z3_string, ty: *mut Z3_sort) -> *mut Z3_ast;
    fn Z3_mk_true(c: *mut Z3_context) -> *mut Z3_ast;
    fn Z3_mk_false(c: *mut Z3_context) -> *mut Z3_ast;
    fn Z3_mk_eq(c: *mut Z3_context, l: *mut Z3_ast, r: *mut Z3_ast) -> *mut Z3_ast;
    fn Z3_mk_lt(c: *mut Z3_context, l: *mut Z3_ast, r: *mut Z3_ast) -> *mut Z3_ast;
    fn Z3_mk_le(c: *mut Z3_context, l: *mut Z3_ast, r: *mut Z3_ast) -> *mut Z3_ast;
    fn Z3_mk_gt(c: *mut Z3_context, l: *mut Z3_ast, r: *mut Z3_ast) -> *mut Z3_ast;
    fn Z3_mk_ge(c: *mut Z3_context, l: *mut Z3_ast, r: *mut Z3_ast) -> *mut Z3_ast;
    fn Z3_mk_not(c: *mut Z3_context, a: *mut Z3_ast) -> *mut Z3_ast;
    fn Z3_mk_and(c: *mut Z3_context, n: u32, args: *const *mut Z3_ast) -> *mut Z3_ast;
    fn Z3_mk_or(c: *mut Z3_context, n: u32, args: *const *mut Z3_ast) -> *mut Z3_ast;
    fn Z3_mk_implies(c: *mut Z3_context, l: *mut Z3_ast, r: *mut Z3_ast) -> *mut Z3_ast;
    fn Z3_mk_add(c: *mut Z3_context, n: u32, args: *const *mut Z3_ast) -> *mut Z3_ast;
    fn Z3_mk_sub(c: *mut Z3_context, n: u32, args: *const *mut Z3_ast) -> *mut Z3_ast;
    fn Z3_mk_mul(c: *mut Z3_context, n: u32, args: *const *mut Z3_ast) -> *mut Z3_ast;
    fn Z3_mk_div(c: *mut Z3_context, l: *mut Z3_ast, r: *mut Z3_ast) -> *mut Z3_ast;
    fn Z3_mk_mod(c: *mut Z3_context, l: *mut Z3_ast, r: *mut Z3_ast) -> *mut Z3_ast;
    fn Z3_mk_select(c: *mut Z3_context, a: *mut Z3_ast, i: *mut Z3_ast) -> *mut Z3_ast;
    fn Z3_mk_store(c: *mut Z3_context, a: *mut Z3_ast, i: *mut Z3_ast, v: *mut Z3_ast) -> *mut Z3_ast;
    fn Z3_mk_const_array(c: *mut Z3_context, ty: *mut Z3_sort, v: *mut Z3_ast) -> *mut Z3_ast;
    fn Z3_mk_ite(c: *mut Z3_context, c: *mut Z3_ast, t: *mut Z3_ast, e: *mut Z3_ast) -> *mut Z3_ast;
    fn Z3_ast_to_string(c: *mut Z3_context, a: *mut Z3_ast) -> Z3_string;
}

fn find_z3_dll() -> Result<std::path::PathBuf, String> {
    // 1. 环境变量
    if let Ok(p) = std::env::var("AETHER_Z3_PATH") {
        let path = std::path::PathBuf::from(&p);
        if path.is_file() {
            return Ok(path);
        }
        return Err(format!("AETHER_Z3_PATH set to '{}' but it is not a file", p));
    }
    // 2. pip z3-solver 用户安装路径
    if let Some(home) = std::env::var_os("USERPROFILE") {
        let mut p = std::path::PathBuf::from(home);
        p.push("AppData/Roaming/Python/Python313/site-packages/z3/lib/libz3.dll");
        if p.is_file() {
            return Ok(p);
        }
    }
    // 3. PATH 中的 libz3.dll / z3.dll
    for name in ["libz3.dll", "z3.dll"] {
        if let Some(dir) = std::env::var_os("PATH") {
            for d in std::env::split_paths(&dir) {
                let p = d.join(name);
                if p.is_file() {
                    return Ok(p);
                }
            }
        }
    }
    Err(
        "Z3 library not found. Install via 'python -m pip install --user z3-solver', or set AETHER_Z3_PATH to libz3.dll"
            .to_string(),
    )
}
