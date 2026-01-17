use crate::rdb::register_info::RegisterFormat::{DoubleFloat, LongDouble};
use crate::rdb::register_info::{Register, RegisterFormat, RegisterId, RegisterType, User};
use std::any::TypeId;
use std::fmt;

pub struct ProcRegisters {
    pub data_: User,
}

impl ProcRegisters {
    pub fn new(user: User) -> Self {
        Self { data_: user }
    }
    pub unsafe fn get_register_val_by_id(&mut self, id: RegisterId) -> Result<RegisterValue, &str> {
        let register = Register::by_id(id);
        self.get_register_val(register)
    }
    unsafe fn get_register_val(&mut self, register: &Register) -> Result<RegisterValue, &str> {
        let bytes = RegisterValue::as_bytes_const(&self.data_);
        let offset = register.offset;

        if register.register_format == RegisterFormat::Uint {
            match register.size {
                1 => {
                    let val = RegisterValue::from_bytes::<u8>(bytes.add(offset));
                    Ok(RegisterValue::U8(val))
                }
                2 => {
                    let val = RegisterValue::from_bytes::<u16>(bytes.add(offset));
                    Ok(RegisterValue::U16(val))
                }
                4 => {
                    let val = RegisterValue::from_bytes::<u32>(bytes.add(offset));
                    Ok(RegisterValue::U32(val))
                }
                8 => {
                    let val = RegisterValue::from_bytes::<u64>(bytes.add(offset));
                    Ok(RegisterValue::U64(val))
                }
                _ => Err("Unexpected register size"),
            }
        } else if register.register_format == DoubleFloat {
            let val = RegisterValue::from_bytes::<f64>(bytes.add(offset));
            Ok(RegisterValue::Double(val))
        } else if register.register_format == LongDouble {
            // Determine which ST register this is
            let st_index = match register.id {
                RegisterId::St0 => 0,
                RegisterId::St1 => 1,
                RegisterId::St2 => 2,
                RegisterId::St3 => 3,
                RegisterId::St4 => 4,
                RegisterId::St5 => 5,
                RegisterId::St6 => 6,
                RegisterId::St7 => 7,
                _ => return Err("Invalid ST register"),
            };

            // Read 16 bytes from st_space (4 u32s per ST register)
            let base_index = st_index * 4;
            let mut x87_bytes = [0u8; 10];

            // Extract the actual 10 bytes of x87 data
            for i in 0..2 {
                let u32_val = self.data_.i387.st_space[base_index + i];
                let bytes_chunk = u32_val.to_le_bytes();
                x87_bytes[i * 4..(i + 1) * 4].copy_from_slice(&bytes_chunk);
            }
            // Get remaining 2 bytes from third u32
            let u32_val = self.data_.i387.st_space[base_index + 2];
            let bytes_chunk = u32_val.to_le_bytes();
            x87_bytes[8..10].copy_from_slice(&bytes_chunk[0..2]);

            Ok(RegisterValue::LongDouble(x87_bytes))
        } else if register.register_format == RegisterFormat::Vector && register.size == 8 {
            let val = RegisterValue::from_bytes::<[u8; 8]>(bytes.add(offset));
            Ok(RegisterValue::Byte64(val))
        } else {
            let val = RegisterValue::from_bytes::<[u8; 16]>(bytes.add(offset));
            Ok(RegisterValue::Byte128(val))
        }
    }

    pub unsafe fn write_register(&mut self, register: &Register, val: RegisterValue) -> *mut u8 {
        let val_size = val.size_of();
        if val_size > register.size {
            return std::ptr::null_mut();
        }

        // Special handling for FPR registers (st_space)
        if register.register_type == RegisterType::Fpr
            && register.register_format == RegisterFormat::LongDouble
        {
            let x87_bytes = match val {
                RegisterValue::LongDouble(bytes) => bytes,
                _ => return std::ptr::null_mut(),
            };

            // Calculate which ST register (0-7)
            let st_index = match register.id {
                RegisterId::St0 => 0,
                RegisterId::St1 => 1,
                RegisterId::St2 => 2,
                RegisterId::St3 => 3,
                RegisterId::St4 => 4,
                RegisterId::St5 => 5,
                RegisterId::St6 => 6,
                RegisterId::St7 => 7,
                _ => return std::ptr::null_mut(),
            };

            // Each ST register = 4 u32s (16 bytes) in st_space, but we only use first 10 bytes
            let base_index = st_index * 4;

            // Convert the 10 bytes to u32 values
            for i in 0..2 {
                let u32_bytes = [
                    x87_bytes[i * 4],
                    x87_bytes[i * 4 + 1],
                    x87_bytes[i * 4 + 2],
                    x87_bytes[i * 4 + 3],
                ];
                self.data_.i387.st_space[base_index + i] = u32::from_le_bytes(u32_bytes);
            }
            // Last 2 bytes go into third u32 (lower 2 bytes)
            let u32_bytes = [x87_bytes[8], x87_bytes[9], 0, 0];
            self.data_.i387.st_space[base_index + 2] = u32::from_le_bytes(u32_bytes);

            // Zero out the padding
            self.data_.i387.st_space[base_index + 3] = 0;

            return &mut self.data_ as *mut _ as *mut u8;
        }

        // Original code for non-FPR registers
        let user_bytes: *mut u8 = RegisterValue::as_bytes_mut(&mut self.data_);
        let reg_offset = register.offset;
        let reg_size = register.size;

        let val_bytes = match val {
            RegisterValue::U8(v) => RegisterValue::widen(register, v),
            RegisterValue::U16(v) => RegisterValue::widen(register, v),
            RegisterValue::U32(v) => RegisterValue::widen(register, v),
            RegisterValue::U64(v) => RegisterValue::widen(register, v),
            RegisterValue::I8(v) => RegisterValue::widen(register, v),
            RegisterValue::I16(v) => RegisterValue::widen(register, v),
            RegisterValue::I32(v) => RegisterValue::widen(register, v),
            RegisterValue::I64(v) => RegisterValue::widen(register, v),
            RegisterValue::Float(v) => RegisterValue::widen(register, v),
            RegisterValue::Double(v) => RegisterValue::widen(register, v),
            RegisterValue::Byte64(v) => RegisterValue::widen(register, v),
            RegisterValue::Byte128(v) => RegisterValue::widen(register, v),
            RegisterValue::LongDouble(v) => RegisterValue::widen(register, v),
            _ => return std::ptr::null_mut(),
        };

        std::ptr::copy_nonoverlapping(val_bytes.as_ptr(), user_bytes.add(reg_offset), reg_size);
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
    LongDouble([u8; 10]),  // Changed from f128::f128
    Byte64([u8; 8]),
    Byte128([u8; 16]),
}

impl fmt::Display for RegisterValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RegisterValue::U8(v) => write!(f, "0x{:02x} ({:3})", v, v),
            RegisterValue::U16(v) => write!(f, "0x{:04x} ({:5})", v, v),
            RegisterValue::U32(v) => write!(f, "0x{:08x} ({:10})", v, v),
            RegisterValue::U64(v) => write!(f, "0x{:016x} ({:20})", v, v),
            RegisterValue::I8(v) => write!(f, "0x{:02x} ({:4})", *v as u8, v),
            RegisterValue::I16(v) => write!(f, "0x{:04x} ({:6})", *v as u16, v),
            RegisterValue::I32(v) => write!(f, "0x{:08x} ({:11})", *v as u32, v),
            RegisterValue::I64(v) => write!(f, "0x{:016x} ({:20})", *v as u64, v),
            RegisterValue::Float(v) => write!(f, "{:e} ({:.6})", v, v),
            RegisterValue::Double(v) => write!(f, "{:e} ({:.15})", v, v),
            RegisterValue::LongDouble(bytes) => {
                // Display as hex bytes and converted f64 value
                write!(f, "[")?;
                for (i, byte) in bytes.iter().enumerate() {
                    if i > 0 {
                        write!(f, ",")?;
                    }
                    write!(f, "0x{:02x}", byte)?;
                }
                write!(f, "]")?;

                // Also show approximate f64 value
                let f64_val = Self::x87_to_f64(bytes);
                write!(f, " ≈ {:.15}", f64_val)
            }
            RegisterValue::Byte64(bytes) => {
                write!(f, "[")?;
                for (i, byte) in bytes.iter().enumerate() {
                    if i > 0 {
                        write!(f, ",")?;
                    }
                    write!(f, "0x{:02x}", byte)?;
                }
                write!(f, "]")
            }
            RegisterValue::Byte128(bytes) => {
                write!(f, "[")?;
                for (i, byte) in bytes.iter().enumerate() {
                    if i > 0 {
                        write!(f, ",")?;
                    }
                    write!(f, "0x{:02x}", byte)?;
                }
                write!(f, "]")
            }
        }
    }
}
impl RegisterValue {
    // Convert x87 10-byte format to f64 for reading
    fn x87_to_f64(bytes: &[u8; 10]) -> f64 {
        // Extract mantissa (8 bytes)
        let mantissa = u64::from_le_bytes([
            bytes[0], bytes[1], bytes[2], bytes[3],
            bytes[4], bytes[5], bytes[6], bytes[7],
        ]);

        // Extract sign and exponent (2 bytes)
        let sign_and_exp = u16::from_le_bytes([bytes[8], bytes[9]]);
        let sign = (sign_and_exp >> 15) & 1;
        let x87_exp = sign_and_exp & 0x7FFF;

        // Check for zero
        if mantissa == 0 && x87_exp == 0 {
            return if sign == 1 { -0.0 } else { 0.0 };
        }

        // Convert x87 to IEEE 754 double
        let ieee_exp = (x87_exp as i32) - 16383 + 1023;

        // Check for overflow/underflow
        if ieee_exp <= 0 {
            return if sign == 1 { -0.0 } else { 0.0 };
        }
        if ieee_exp >= 0x7FF {
            return if sign == 1 { f64::NEG_INFINITY } else { f64::INFINITY };
        }

        // Extract top 52 bits of mantissa (remove explicit leading 1)
        let ieee_mantissa = (mantissa << 1) >> 12;

        // Construct IEEE 754 double
        let ieee_bits = ((sign as u64) << 63)
            | ((ieee_exp as u64) << 52)
            | (ieee_mantissa & 0xFFFFFFFFFFFFF);

        f64::from_bits(ieee_bits)
    }

    // Convert f64 to x87 10-byte format for writing
    pub(crate) fn f64_to_x87(val: f64) -> [u8; 10] {
        let mut result = [0u8; 10];

        if val == 0.0 {
            return result;
        }

        let bits = val.to_bits();
        let sign = (bits >> 63) & 1;
        let exp = ((bits >> 52) & 0x7FF) as i32;
        let mantissa = bits & 0xFFFFFFFFFFFFF;

        if exp == 0 {
            return result;
        }

        if exp == 0x7FF {
            return result;
        }

        let x87_exp = (exp - 1023 + 16383) as u16;
        let x87_mantissa = (1u64 << 63) | (mantissa << 11);

        // Layout: [mantissa: 8 bytes][sign+exp: 2 bytes]
        result[0..8].copy_from_slice(&x87_mantissa.to_le_bytes());

        let sign_and_exp = ((sign as u16) << 15) | (x87_exp & 0x7FFF);
        result[8..10].copy_from_slice(&sign_and_exp.to_le_bytes());

        result
    }
    pub fn size_of(&self) -> usize {
        match self {
            RegisterValue::U8(_) | RegisterValue::I8(_) => 1,
            RegisterValue::U16(_) | RegisterValue::I16(_) => 2,
            RegisterValue::U32(_) | RegisterValue::I32(_) | RegisterValue::Float(_) => 4,
            RegisterValue::U64(_) | RegisterValue::I64(_) | RegisterValue::Double(_) => 8,
            RegisterValue::LongDouble(_) => 10, // x87 80-bit
            RegisterValue::Byte64(_) => 8,
            RegisterValue::Byte128(_) => 16,
        }
    }
    pub fn as_bytes_mut<From>(obj: &mut From) -> *mut u8 {
        obj as *mut From as *mut u8
    }
    pub fn as_bytes_const<From>(obj: &From) -> *const u8 {
        obj as *const From as *const u8
    }
    pub unsafe fn from_bytes<To>(bytes: *const u8) -> To {
        let mut ret: To = std::mem::zeroed();
        std::ptr::copy_nonoverlapping(bytes, &mut ret as *mut To as *mut u8, size_of::<To>());
        ret
    }
    // Updated widen function
    pub unsafe fn widen<From: 'static>(register: &Register, t: From) -> [u8; 16] {
        if RegisterValue::is_floating_point::<From>() {
            if register.register_format == RegisterFormat::DoubleFloat {
                let val = std::mem::transmute_copy::<From, f64>(&t);
                RegisterValue::to_byte128(val)
            } else if register.register_format == RegisterFormat::LongDouble {
                // For LongDouble with [u8; 10] input
                if std::any::TypeId::of::<From>() == std::any::TypeId::of::<[u8; 10]>() {
                    let bytes = std::mem::transmute_copy::<From, [u8; 10]>(&t);
                    let mut result = [0u8; 16];
                    result[0..10].copy_from_slice(&bytes);
                    return result;
                }
                // For f64 input, convert to x87
                let val = std::mem::transmute_copy::<From, f64>(&t);
                let x87_bytes = RegisterValue::f64_to_x87(val);
                let mut result = [0u8; 16];
                result[0..10].copy_from_slice(&x87_bytes);
                result
            } else {
                RegisterValue::to_byte128(t)
            }
        } else if RegisterValue::is_signed::<From>() {
            match register.size {
                2 => {
                    let val = std::mem::transmute_copy::<From, i16>(&t);
                    RegisterValue::to_byte128(val)
                }
                4 => {
                    let val = std::mem::transmute_copy::<From, i32>(&t);
                    RegisterValue::to_byte128(val)
                }
                8 => {
                    let val = std::mem::transmute_copy::<From, i64>(&t);
                    RegisterValue::to_byte128(val)
                }
                _ => RegisterValue::to_byte128(t),
            }
        } else {
            RegisterValue::to_byte128(t)
        }
    }

    fn is_floating_point<T: 'static>() -> bool {
        let tid = std::any::TypeId::of::<T>();
        tid == std::any::TypeId::of::<f32>()
            || tid == std::any::TypeId::of::<f64>()
            || tid == std::any::TypeId::of::<[u8; 10]>()
    }
    unsafe fn to_byte128<From>(bytes: From) -> [u8; 16] {
        let mut result = [0u8; 16];
        let ptr = &bytes as *const From as *const u8;
        let size = std::mem::size_of::<From>();
        std::ptr::copy_nonoverlapping(ptr, result.as_mut_ptr(), size.min(16));
        result
    }
    fn is_signed<From: 'static>() -> bool {
        use std::any::TypeId;
        let tid = TypeId::of::<From>();
        tid == TypeId::of::<i16>() || tid == TypeId::of::<i32>() || tid == TypeId::of::<i64>()
    }
}
