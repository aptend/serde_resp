use serde::ser::{self, Serialize};

use super::error::{Error, Result};

pub struct Serializer {
    // 满足 redis protocol 的命令输出，以*开头
    output: Vec<u8>,
}

// 按照约定，序列化的接口为 to_string, to_bytes, to_writer
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
    // Serializer添加bulk String的函数
    fn append_element(&mut self, element: &[u8]) {
        self.output
            .extend_from_slice(&format!("${}\r\n", element.len()).as_bytes());
        self.output.extend_from_slice(element);
        self.output.push(b'\r');
        self.output.push(b'\n');
    }
}

impl<'a> ser::Serializer for &'a mut Serializer {
    // The output type produced by this `Serializer` during successful
    // serialization. Most serializers that produce text or binary output should
    // set `Ok = ()` and serialize into an `io::Write` or buffer contained
    // within the `Serializer` instance, as happens here. Serializers that build
    // in-memory data structures may be simplified by using `Ok` to propagate
    // the data structure around.
    type Ok = ();

    // The error type when some error occurs during serialization.
    type Error = Error;

    // Associated types for keeping track of additional state while serializing
    // compound data structures like sequences and maps. In this case no
    // additional state is required beyond what is already stored in the
    // Serializer struct.
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

    fn serialize_f32(self, v: f32) -> Result<()> {
        self.serialize_f64(f64::from(v))
    }

    fn serialize_f64(self, v: f64) -> Result<()> {
        self.append_element(&v.to_string().as_bytes());
        Ok(())
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

    // 对于struct，当成集合类型，把它处理一个单独的resp命令
    // 比如 struct Quit， 表明这个array长度为1
    fn serialize_unit_struct(self, _name: &'static str) -> Result<()> {
        self.output.extend_from_slice(b"*1\r\n");
        self.serialize_str(_name)
    }

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

    // 官方鼓励 serializer 把 newtype structs 仅仅当作特定数据的简单包装， 直接序列化
    // 被包装的value就可以
    // 但是延续之前对unit struct的处理，这里把newtype struct当作拥有一个参数的命令更为
    // 合理
    fn serialize_newtype_struct<T>(self, _name: &'static str, value: &T) -> Result<()>
    where
        T: ?Sized + Serialize,
    {
        self.output.extend_from_slice(b"*2\r\n");
        self.serialize_str(_name)?;
        value.serialize(self)
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

    // 现在来处理复合类型的序列化
    //
    // 列表的序列化，因为resp要求长度前置，所以如果集合没有长度就报错
    // 否则写入列表的起始头
    fn serialize_seq(self, _len: Option<usize>) -> Result<Self::SerializeSeq> {
        match _len {
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

    // tuple struct 要延续我们之前对newtype struct的处理，_name作为第一项
    fn serialize_tuple_struct(
        self,
        _name: &'static str,
        len: usize,
    ) -> Result<Self::SerializeTupleStruct> {
        let tuple = self.serialize_seq(Some(len + 1))?;
        tuple.serialize_str(_name)?;
        Ok(tuple)
    }

    fn serialize_tuple_variant(
        self,
        _name: &'static str,
        _variant_index: u32,
        variant: &'static str,
        _len: usize,
    ) -> Result<Self::SerializeTupleVariant> {
        self.serialize_tuple_struct(variant, _len)
    }

    fn serialize_struct(self, _name: &'static str, len: usize) -> Result<Self::SerializeStruct> {
        self.serialize_tuple_struct(_name, len)
    }

    fn serialize_struct_variant(
        self,
        _name: &'static str,
        _variant_index: u32,
        variant: &'static str,
        _len: usize,
    ) -> Result<Self::SerializeStructVariant> {
        self.serialize_tuple_struct(variant, _len)
    }

    // Map 在 resp 中表示为多个命令。但是顺序无法保证，考虑直接报错？
    fn serialize_map(self, _len: Option<usize>) -> Result<Self::SerializeMap> {
        unimplemented!("map type is not supported")
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

// Some `Serialize` types are not able to hold a key and value in memory at the
// same time so `SerializeMap` implementations are required to support
// `serialize_key` and `serialize_value` individually.
//
// There is a third optional method on the `SerializeMap` trait. The
// `serialize_entry` method allows serializers to optimize for the case where
// key and value are both available simultaneously. In JSON it doesn't make a
// difference so the default behavior for `serialize_entry` is fine.
impl<'a> ser::SerializeMap for &'a mut Serializer {
    type Ok = ();
    type Error = Error;

    // The Serde data model allows map keys to be any serializable type. JSON
    // only allows string keys so the implementation below will produce invalid
    // JSON if the key serializes as something other than a string.
    //
    // A real JSON serializer would need to validate that map keys are strings.
    // This can be done by using a different Serializer to serialize the key
    // (instead of `&mut **self`) and having that other serializer only
    // implement `serialize_str` and return an error on any other data type.
    fn serialize_key<T>(&mut self, key: &T) -> Result<()>
    where
        T: ?Sized + Serialize,
    {
        key.serialize(&mut **self)
    }

    // It doesn't make a difference whether the colon is printed at the end of
    // `serialize_key` or at the beginning of `serialize_value`. In this case
    // the code is a bit simpler having it here.
    fn serialize_value<T>(&mut self, value: &T) -> Result<()>
    where
        T: ?Sized + Serialize,
    {
        value.serialize(&mut **self)
    }

    fn end(self) -> Result<()> {
        Ok(())
    }
}

////////////////////////////////////////////////////////////////////////////////

#[test]
fn test_struct() {
    #[derive(serde::Serialize)]
    struct Test {
        int: u32,
        seq: Vec<&'static str>,
    }

    let test = Test {
        int: 1,
        seq: vec!["a", "b"],
    };
    let expected = "*3\r\n$4\r\nTest\r\n$1\r\n1\r\n*2\r\n$1\r\na\r\n$1\r\nb\r\n".as_bytes();
    assert_eq!(to_bytes(&test).unwrap(), expected);
}

#[test]
fn test_enum() {
    #[derive(serde::Serialize)]
    enum Test {
        Unit,
        Newtype(u32),
        Tuple(u32, u32),
        Struct { a: u32 },
    }

    let u = Test::Unit;
    assert_eq!(to_bytes(&u).unwrap(), b"*1\r\n$4\r\nUnit\r\n");

    let n = Test::Newtype(1);
    assert_eq!(to_bytes(&n).unwrap(), b"*2\r\n$7\r\nNewtype\r\n$1\r\n1\r\n");

    let t = Test::Tuple(1, 2);
    assert_eq!(
        to_bytes(&t).unwrap(),
        b"*3\r\n$5\r\nTuple\r\n$1\r\n1\r\n$1\r\n2\r\n"
    );

    let s = Test::Struct { a: 1 };
    assert_eq!(to_bytes(&s).unwrap(), b"*2\r\n$6\r\nStruct\r\n$1\r\n1\r\n");
}
