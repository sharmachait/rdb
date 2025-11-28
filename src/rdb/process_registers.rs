use std::any::TypeId;
use crate::rdb::register_info::{Register, RegisterFormat, RegisterId, User};
use crate::rdb::register_info::RegisterFormat::{DoubleFloat, LongDouble};

pub struct ProcRegisters {
    pub data_: User
}

impl ProcRegisters {
    pub fn new(user: User)-> Self{
        Self {
            data_: user
        }
    }
    pub unsafe fn get_register_val_by_id(
        &mut self,
        id: RegisterId
    ) -> Result<RegisterValue, &str> {
        let register = Register::by_id(id);
        self.get_register_val(register)
    }
    unsafe fn get_register_val(&mut self, register: &Register) -> Result<RegisterValue, &str> {
        let bytes = RegisterValue::as_bytes_const(&self.data_);
        let offset = register.offset;
        if register.register_format == RegisterFormat::Uint {
            match register.size {
                1 =>  {
                    let val = RegisterValue::from_bytes::<u8>(bytes.add(offset));
                    Ok(RegisterValue::U8(val))
                },
                2 => {
                    let val = RegisterValue::from_bytes::<u16>(bytes.add(offset));
                    Ok(RegisterValue::U16(val))
                },
                4 => {
                    let val = RegisterValue::from_bytes::<u32>(bytes.add(offset));
                    Ok(RegisterValue::U32(val))
                },
                8 => {
                    let val = RegisterValue::from_bytes::<u64>(bytes.add(offset));
                    Ok(RegisterValue::U64(val))
                },
                _ => {
                    Err("Unexepected register size")
                }
            }
        } else if register.register_format == DoubleFloat {
            let val = RegisterValue::from_bytes::<f64>(bytes.add(offset));
            Ok(RegisterValue::Double(val))
        } else if register.register_format == LongDouble {
            let val =  RegisterValue::from_bytes::<f128::f128>(bytes.add(offset));
            Ok(RegisterValue::LongDouble(val))
        } else if register.register_format == RegisterFormat::Vector && register.size == 8 {
            let val =  RegisterValue::from_bytes::<[u8;8]>(bytes.add(offset));
            Ok(RegisterValue::Byte64(val))
        }else {
            let val =  RegisterValue::from_bytes::<[u8;16]>(bytes.add(offset));
            Ok(RegisterValue::Byte128(val))
        }
    }
    pub unsafe fn write_register(
        &mut self,
        register: &Register,
        val :RegisterValue
    ) -> *mut u8 {
        let user_bytes: *mut u8 = RegisterValue::as_bytes_mut( &mut self.data_);
        let reg_offset = register.offset;
        let reg_size = register.size;

        let val_bytes = match val {
            RegisterValue::U8(v) => {RegisterValue::widen(register, v)}
            RegisterValue::U16(v) => {RegisterValue::widen(register, v)}
            RegisterValue::U32(v) => {RegisterValue::widen(register, v)}
            RegisterValue::U64(v) => {RegisterValue::widen(register, v)}
            RegisterValue::I8(v) => {RegisterValue::widen(register, v)}
            RegisterValue::I16(v) => {RegisterValue::widen(register, v)}
            RegisterValue::I32(v) => {RegisterValue::widen(register, v)}
            RegisterValue::I64(v) => {RegisterValue::widen(register, v)}
            RegisterValue::Float(v) => {RegisterValue::widen(register, v)}
            RegisterValue::Double(v) => {RegisterValue::widen(register, v)}
            RegisterValue::LongDouble(v) => {RegisterValue::widen(register, v)}
            RegisterValue::Byte64(v) => {RegisterValue::widen(register, v)}
            RegisterValue::Byte128(v) => {RegisterValue::widen(register, v)}
        };

        std::ptr::copy_nonoverlapping(
            val_bytes.as_ptr(),
            user_bytes.add(reg_offset),
            reg_size
        );
        user_bytes
    }
}
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum RegisterValue {
    U8(u8),
    U16(u16),
    U32(u32),
    U64(u64),
    I8(i8),
    I16(i16),
    I32(i32),
    I64(i64),
    Float(f32),
    Double(f64),
    LongDouble(f128::f128),
    Byte64([u8; 8]),
    Byte128([u8; 16]),
}

impl RegisterValue {
    pub fn as_bytes_mut<From>(obj: &mut From) -> *mut u8{
        obj as *mut From as *mut u8
    }
    pub fn as_bytes_const<From>(obj: &From) -> *const u8{
        obj as *const From as *const u8
    }
    pub unsafe fn from_bytes<To>(bytes: *const u8) -> To {
        let mut ret :To = std::mem::zeroed();
        std::ptr::copy_nonoverlapping(
            bytes,
            &mut ret as *mut To as *mut u8,
            size_of::<To>()
        );
        ret
    }
    pub unsafe fn widen<From: 'static>(register: &Register, t: From) -> [u8; 16] {
        if RegisterValue::is_floating_point::<From>() {
            if register.register_format == RegisterFormat::DoubleFloat {
                let val = std::mem::transmute_copy::<From, f64>(&t);
                 RegisterValue::to_byte128(val)
            }else if register.register_format == RegisterFormat::LongDouble {
                let val = std::mem::transmute_copy::<From, f128::f128>(&t);
                 RegisterValue::to_byte128(val)
            }else{
                 RegisterValue::to_byte128(t)
            }
        } else if RegisterValue::is_signed::<From>() {
            match register.size {
                2=> {
                    let val = std::mem::transmute_copy::<From, i16>(&t);
                    RegisterValue::to_byte128(val)
                }
                4=> {
                    let val = std::mem::transmute_copy::<From, i32>(&t);
                    RegisterValue::to_byte128(val)
                }
                8 => {
                    let val = std::mem::transmute_copy::<From, i64>(&t);
                    RegisterValue::to_byte128(val)
                }
                _ => {
                    RegisterValue::to_byte128(t)
                }
            }
        }else{
            RegisterValue::to_byte128(t)
        }
    }
    fn is_floating_point<T: 'static>() -> bool {
        use std::any::TypeId;
        let tid = TypeId::of::<T>();
        tid == TypeId::of::<f32>() ||
            tid == TypeId::of::<f64>() ||
            tid == TypeId::of::<f128::f128>()
    }
    unsafe fn to_byte128<From>(bytes: From) -> [u8; 16] {
        let mut result = [0u8; 16];
        let ptr = &bytes as *const From as *const u8;
        let size = std::mem::size_of::<From>();
        std::ptr::copy_nonoverlapping(
            ptr,
            result.as_mut_ptr(),
            size.min(16)
        );
        result
    }
    fn is_signed<From: 'static>() -> bool {
        use std::any::TypeId;
        let tid = TypeId::of::<From>();
        tid == TypeId::of::<i16>() ||
            tid == TypeId::of::<i32>() ||
            tid == TypeId::of::<i64>()
    }
}