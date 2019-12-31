use serde::ser::{self, Serialize};

use super::error::{Error, Result};

pub struct Serializer {
    // 满足 redis protocol 的命令输出，以*开头
    output: Vec<u8>,
}

// Redis Simple Protocol规定，发往服务端的信息，是bulk string，这里用bytes来表示
pub fn to_bytes<T>(value: &T) -> Result<Vec<u8>>
where
    T: Serialize,
{
    let mut serializer = Serializer { output: vec![] };
    value.serialize(&mut serializer)?;
    Ok(serializer.output)
}

impl Serializer {
    // Serializer添加bulk String的helper
    fn append_element(&mut self, element: &[u8]) {
        self.output
            .extend_from_slice(&format!("${}\r\n", element.len()).as_bytes());
        self.output.extend_from_slice(element);
        self.output.push(b'\r');
        self.output.push(b'\n');
    }
}

impl<'a> ser::Serializer for &'a mut Serializer {
    type Ok = ();
    type Error = Error;

    type SerializeSeq = Self;
    type SerializeTuple = Self;
    type SerializeTupleStruct = Self;
    type SerializeTupleVariant = Self;
    type SerializeMap = Self;
    type SerializeStruct = Self;
    type SerializeStructVariant = Self;

    // 首先从简单的方法开始。 以下12个方法，接受一个基本类型，映射为resp的一个bulk string
    fn serialize_bool(self, v: bool) -> Result<()> {
        self.append_element(if v { b"true" } else { b"false" });
        Ok(())
    }

    // resp的列表元素对整数类型不敏感
    fn serialize_i8(self, v: i8) -> Result<()> {
        self.serialize_i64(i64::from(v))
    }

    fn serialize_i16(self, v: i16) -> Result<()> {
        self.serialize_i64(i64::from(v))
    }

    fn serialize_i32(self, v: i32) -> Result<()> {
        self.serialize_i64(i64::from(v))
    }

    // 这里如果要追求性能，应该使用`itoa` crate，而不是to_string
    fn serialize_i64(self, v: i64) -> Result<()> {
        self.append_element(&v.to_string().as_bytes());
        Ok(())
    }

    fn serialize_u8(self, v: u8) -> Result<()> {
        self.serialize_u64(u64::from(v))
    }

    fn serialize_u16(self, v: u16) -> Result<()> {
        self.serialize_u64(u64::from(v))
    }

    fn serialize_u32(self, v: u32) -> Result<()> {
        self.serialize_u64(u64::from(v))
    }

    fn serialize_u64(self, v: u64) -> Result<()> {
        self.append_element(&v.to_string().as_bytes());
        Ok(())
    }

    fn serialize_f32(self, _v: f32) -> Result<()> {
        Err(Error::Message("float is not supported".to_owned()))
    }

    fn serialize_f64(self, _v: f64) -> Result<()> {
        Err(Error::Message("float is not supported".to_owned()))
    }

    // 单个字符也被当做字符串
    fn serialize_char(self, v: char) -> Result<()> {
        self.serialize_str(&v.to_string())
    }

    // 直接把字符串转为bytes，没有转义
    fn serialize_str(self, v: &str) -> Result<()> {
        self.serialize_bytes(v.as_bytes())
    }

    // bytes当作列表元素
    fn serialize_bytes(self, v: &[u8]) -> Result<()> {
        self.append_element(v);
        Ok(())
    }

    // 空值，null bulk string $-1\r\n表示
    fn serialize_none(self) -> Result<()> {
        self.serialize_unit()
    }

    // Some(()) 也会被当作None处理，resp的表达能力还不够高
    fn serialize_some<T>(self, value: &T) -> Result<()>
    where
        T: ?Sized + Serialize,
    {
        value.serialize(self)
    }

    // 空值，null bulk string $-1\r\n表示
    fn serialize_unit(self) -> Result<()> {
        self.output.extend_from_slice(b"$-1\r\n");
        Ok(())
    }

    // 列表的序列化，因为resp要求长度前置，所以如果集合没有长度就报错
    // 否则写入列表的起始头
    fn serialize_seq(self, len: Option<usize>) -> Result<Self::SerializeSeq> {
        match len {
            None => Err(Error::Message(
                "length of sequence can't be determined".to_owned(),
            )),
            Some(l) => {
                self.output
                    .extend_from_slice(format!("*{}\r\n", l).as_bytes());
                Ok(self)
            }
        }
    }

    // tuples和列表基本相同，但是它的长度是确定的
    fn serialize_tuple(self, len: usize) -> Result<Self::SerializeTuple> {
        self.serialize_seq(Some(len))
    }

    ///////////////////////////////////////// struct

    // 对于struct，当成集合类型，把它处理一个单独的resp命令
    // 形如struct Foo; 可以看成无参数命令
    fn serialize_unit_struct(self, name: &'static str) -> Result<()> {
        self.output.extend_from_slice(b"*1\r\n");
        self.serialize_str(name)
    }

    // 官方鼓励 serializer 把 newtype structs 仅仅当作特定数据的简单包装，直接序列化
    // 被包装的value就可以
    // 但是延续之前对unit struct的处理，这里把newtype struct当作拥有一个参数的命令
    // newtype_struct，形如struct Foo(i32);，单一参数命令
    fn serialize_newtype_struct<T>(self, name: &'static str, value: &T) -> Result<()>
    where
        T: ?Sized + Serialize,
    {
        self.output.extend_from_slice(b"*2\r\n");
        self.serialize_str(name)?;
        value.serialize(self)
    }

    // tuple_struct，形如struct Foo(i32, i32, i32); 多参数命令，总长度为参数长度+1
    fn serialize_tuple_struct(
        self,
        name: &'static str,
        len: usize,
    ) -> Result<Self::SerializeTupleStruct> {
        let tuple = self.serialize_seq(Some(len + 1))?;
        tuple.serialize_str(name)?;
        Ok(tuple)
    }

    fn serialize_struct(self, name: &'static str, len: usize) -> Result<Self::SerializeStruct> {
        self.serialize_tuple_struct(name, len)
    }

    ///////////////////////////////////////// enum

    // 在处理unit的枚举时，binary格式一般使用索引表示，注重可读性的格式则会使用名称
    // 这里实现当作unit struct处理
    fn serialize_unit_variant(
        self,
        _name: &'static str,
        _variant_index: u32,
        variant: &'static str,
    ) -> Result<()> {
        self.serialize_unit_struct(variant)
    }

    // 枚举 newtype struct， 所以直接扔给 newtype struct 处理
    fn serialize_newtype_variant<T>(
        self,
        _name: &'static str,
        _variant_index: u32,
        variant: &'static str,
        value: &T,
    ) -> Result<()>
    where
        T: ?Sized + Serialize,
    {
        self.serialize_newtype_struct(variant, value)
    }

    fn serialize_tuple_variant(
        self,
        _name: &'static str,
        _variant_index: u32,
        variant: &'static str,
        len: usize,
    ) -> Result<Self::SerializeTupleVariant> {
        self.serialize_tuple_struct(variant, len)
    }

    fn serialize_struct_variant(
        self,
        _name: &'static str,
        _variant_index: u32,
        variant: &'static str,
        len: usize,
    ) -> Result<Self::SerializeStructVariant> {
        self.serialize_tuple_struct(variant, len)
    }

    // Map 在 resp 中表示为多个命令。但是顺序无法保证
    fn serialize_map(self, _len: Option<usize>) -> Result<Self::SerializeMap> {
        Err(Error::Message("float is not supported".to_owned()))
    }
}

// 下列7个实现，处理例如 seq 和 map 的序列化。一般由Serializer发起，然后调用若干个
// element的序列化方法，最后以一个end结尾
//
// serialize_seq后返回当前实现
impl<'a> ser::SerializeSeq for &'a mut Serializer {
    // 和 the serializer 的Ok类型一致.
    type Ok = ();
    // 和 the serializer 的Error类型一致.
    type Error = Error;

    fn serialize_element<T>(&mut self, value: &T) -> Result<()>
    where
        T: ?Sized + Serialize,
    {
        // resp 格式，直接往后添加就可以了
        value.serialize(&mut **self)
    }

    // 关闭时什么都不用做
    fn end(self) -> Result<()> {
        Ok(())
    }
}

// tuples和seq一样
impl<'a> ser::SerializeTuple for &'a mut Serializer {
    type Ok = ();
    type Error = Error;

    fn serialize_element<T>(&mut self, value: &T) -> Result<()>
    where
        T: ?Sized + Serialize,
    {
        value.serialize(&mut **self)
    }

    fn end(self) -> Result<()> {
        Ok(())
    }
}

// 同上.
impl<'a> ser::SerializeTupleStruct for &'a mut Serializer {
    type Ok = ();
    type Error = Error;

    fn serialize_field<T>(&mut self, value: &T) -> Result<()>
    where
        T: ?Sized + Serialize,
    {
        value.serialize(&mut **self)
    }

    fn end(self) -> Result<()> {
        Ok(())
    }
}

// 同上
impl<'a> ser::SerializeTupleVariant for &'a mut Serializer {
    type Ok = ();
    type Error = Error;

    fn serialize_field<T>(&mut self, value: &T) -> Result<()>
    where
        T: ?Sized + Serialize,
    {
        value.serialize(&mut **self)
    }

    fn end(self) -> Result<()> {
        Ok(())
    }
}

// 把Struct的枚举当作Tuple, 忽略key，直接取数据，当作tuple
impl<'a> ser::SerializeStruct for &'a mut Serializer {
    type Ok = ();
    type Error = Error;

    fn serialize_field<T>(&mut self, _key: &'static str, value: &T) -> Result<()>
    where
        T: ?Sized + Serialize,
    {
        value.serialize(&mut **self)
    }

    fn end(self) -> Result<()> {
        Ok(())
    }
}

// 同上
impl<'a> ser::SerializeStructVariant for &'a mut Serializer {
    type Ok = ();
    type Error = Error;

    fn serialize_field<T>(&mut self, _key: &'static str, value: &T) -> Result<()>
    where
        T: ?Sized + Serialize,
    {
        value.serialize(&mut **self)
    }

    fn end(self) -> Result<()> {
        Ok(())
    }
}

impl<'a> ser::SerializeMap for &'a mut Serializer {
    type Ok = ();
    type Error = Error;

    fn serialize_key<T>(&mut self, _key: &T) -> Result<()>
    where
        T: ?Sized + Serialize,
    {
        unimplemented!();
    }

    fn serialize_value<T>(&mut self, _value: &T) -> Result<()>
    where
        T: ?Sized + Serialize,
    {
        unimplemented!();
    }

    fn end(self) -> Result<()> {
        Ok(())
    }
}
