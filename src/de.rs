use std::ops::{AddAssign, MulAssign, Neg};

use serde::de::{
    self, Deserialize, DeserializeSeed, EnumAccess, SeqAccess, VariantAccess, Visitor,
};

use super::error::{Error, Result};

use std::io::{self, Read, BufRead};

use std::marker::PhantomData;

const CR: u8 = b'\r';
const LF: u8 = b'\n';

pub struct Deserializer<'de, R> {
    reader: io::BufReader<R>,
    byte_offset: usize,
    marker: PhantomData<&'de ()>,
}


// 暴露的公共API，表明反序列化要用的数据格式，形如 from_xxx
pub fn from_bytes<'a, T>(s: &'a [u8]) -> Result<T>
where
    T: Deserialize<'a>,
{
    let mut deserializer = Deserializer::from_reader(s);
    let t = T::deserialize(&mut deserializer)?;
    Ok(t)
    // if deserializer.input.is_empty() {
    //     Ok(t)
    // } else {
    //     Err(Error::TrailingBytes)
    // }
}

impl<'de, R: io::Read> Deserializer<'de, R> {

    pub fn from_reader(r: R) -> Self {
        Deserializer {
            reader: io::BufReader::new(r),
            byte_offset: 0,
            marker: PhantomData,
        }
    }

    // 查看第一个u8
    fn peek_char(&mut self) -> Result<u8> {
        Ok(self.peek_nchar(1)?[0])
    }

    fn peek_nchar(&mut self, n: usize) -> Result<&[u8]> {
        while self.reader.buffer().len() < n {
            if self.reader.fill_buf()?.len() == 0 {
                return Err(Error::Eof);
            }
        }
        Ok(&self.reader.buffer()[0..n])
    }

    fn consume(&mut self, n: usize) {
        self.reader.consume(n)
    }

    fn next_char(&mut self) -> Result<u8> {
        let ch = self.peek_char()?;
        self.consume(1);
        self.byte_offset += 1;
        Ok(ch)
    }

    fn next_lf(&mut self) -> Result<Vec<u8>> {
        let mut buf = vec![];
        let n = self.reader.read_until(LF, &mut buf)?;
        if n == 0 {
            return Err(Error::Eof);
        }

        if n < 2 || buf[n - 2] != CR {
            return Err(Error::UnbalancedCRLF);
        }

        Ok(buf)
    }


    fn next_length_hint(&mut self) -> Result<Option<usize>> {
        let buf = self.next_lf()?;
        let n = buf.len();
        if buf[0] == b'-' {
            if buf.len() == 4 || buf == b"-1\r\n" {
                return Ok(None)
            } else {
                return Err(Error::BadLengthHint);
            }
        }
        let mut len = 0;
        for &ch in buf.iter().take(n-2) {
            match ch {
                ch @ b'0'..=b'9' => {
                    len *= 10;
                    len += usize::from(ch as u8 - b'0');
                }
                _ => return Err(Error::BadLengthHint),
            }
        }
        self.byte_offset += n;
        Ok(Some(len))
    }


    fn parse_bulk_string(&mut self) -> Result<Option<Vec<u8>>> {
        if self.next_char()? != b'$' {
            return Err(Error::ExpectedDollarSign);
        }
        match self.next_length_hint()? {
            Some(len) => {
                let mut buf = vec![0; len+2];
                self.reader.read_exact(&mut buf)?;
                if buf[len + 1] != LF {
                    return Err(Error::ExpectedLF);
                }
                if buf[len] != CR {
                    return Err(Error::UnbalancedCRLF);
                }
                self.byte_offset += len + 2;
                Ok(Some(buf))
            }
            None => Ok(None),
        }
    }

    fn parse_bool(&mut self) -> Result<bool> {
        if self.peek_nchar(10)? == b"$4\r\ntrue\r\n" {
            self.consume(10);
            Ok(true)
        } else if self.peek_nchar(11)? == b"$5\r\nfalse\r\n" {
            self.consume(11);
            Ok(false)
        } else {
            Err(Error::ExpectedBoolean)
        }
    }

    fn parse_unsigned<T>(&mut self) -> Result<T>
    where
        T: AddAssign<T> + MulAssign<T> + From<u8>,
    {
        match self.parse_bulk_string()? {
            Some(num_bytes) => {
                let mut num = T::from(0);
                for ch in num_bytes {
                    match ch {
                        ch @ b'0'..=b'9' => {
                            num *= T::from(10);
                            num += T::from(ch - b'0');
                        }
                        _ => return Err(Error::BadNumContent),
                    }
                }
                Ok(num)
            },
            None => Err(Error::BadNumContent),
        }
    }


    fn parse_signed<T>(&mut self) -> Result<T>
    where
        T: Neg<Output = T> + AddAssign<T> + MulAssign<T> + From<i8>,
    {
        match self.parse_bulk_string()? {
            Some(num_bytes) => {
                let mut neg = false;
                let mut skip = 0;
                if num_bytes.len() > 0 && num_bytes[0] == b'-' {
                    neg = true;
                    skip = 1;
                }
                let mut num = T::from(0);
                for &ch in num_bytes.iter().skip(skip) {
                    match ch {
                        ch @ b'0'..=b'9' => {
                            num *= T::from(10);
                            num += T::from(ch as i8 - b'0' as i8);
                        }
                        _ => return Err(Error::BadNumContent),
                    }
                }
                Ok(if neg { -num } else {num})
            }
            None => Err(Error::BadNumContent),
        }
    }
}

impl<'de, 'a, R: io::Read> de::Deserializer<'de> for &'a mut Deserializer<'de, R> {
    type Error = Error;

    // Look at the input data to decide what Serde data model type to
    // deserialize as. Not all data formats are able to support this operation.
    // Formats that support `deserialize_any` are known as self-describing.
    fn deserialize_any<V>(self, _visitor: V) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        unimplemented!()
    }
    fn deserialize_ignored_any<V>(self, visitor: V) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        self.deserialize_any(visitor)
    }

    fn deserialize_bool<V>(self, visitor: V) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        visitor.visit_bool(self.parse_bool()?)
    }

    fn deserialize_i8<V>(self, visitor: V) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        visitor.visit_i8(self.parse_signed()?)
    }

    fn deserialize_i16<V>(self, visitor: V) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        visitor.visit_i16(self.parse_signed()?)
    }

    fn deserialize_i32<V>(self, visitor: V) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        visitor.visit_i32(self.parse_signed()?)
    }

    fn deserialize_i64<V>(self, visitor: V) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        visitor.visit_i64(self.parse_signed()?)
    }

    fn deserialize_u8<V>(self, visitor: V) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        visitor.visit_u8(self.parse_unsigned()?)
    }

    fn deserialize_u16<V>(self, visitor: V) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        visitor.visit_u16(self.parse_unsigned()?)
    }

    fn deserialize_u32<V>(self, visitor: V) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        visitor.visit_u32(self.parse_unsigned()?)
    }

    fn deserialize_u64<V>(self, visitor: V) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        visitor.visit_u64(self.parse_unsigned()?)
    }

    // Float parsing is stupidly hard.
    // 浮点数的解析，直译，蠢难蠢难的 😂， 放弃了
    fn deserialize_f32<V>(self, _visitor: V) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        unimplemented!()
    }

    fn deserialize_f64<V>(self, _visitor: V) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        unimplemented!()
    }

    fn deserialize_char<V>(self, _visitor: V) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        // Parse a string, check that it is one character, call `visit_char`.
        unimplemented!()
    }

    // 对于 str 直接给 bytes, 用 visitor.visit_borrowed_bytes 去构建
    fn deserialize_str<V>(self, visitor: V) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        self.deserialize_bytes(visitor)
    }

    fn deserialize_string<V>(self, visitor: V) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        self.deserialize_str(visitor)
    }


    fn deserialize_bytes<V>(self, visitor: V) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        let s = self.parse_bulk_string()?.unwrap();
        visitor.visit_bytes(&s[..])
    }

    fn deserialize_byte_buf<V>(self, _visitor: V) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        unimplemented!()
    }

    fn deserialize_option<V>(self, visitor: V) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        if self.peek_nchar(5)? == b"$-1\r\n" {
            self.consume(5);
            visitor.visit_none()
        } else {
            visitor.visit_some(self)
        }
    }

    fn deserialize_unit<V>(self, visitor: V) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        match self.parse_bulk_string()? {
            Some(_) => Err(Error::ExpectedNone),
            None => visitor.visit_unit(),
        }
    }

    // unit struct 代表无参数命令, 检查是否和数据匹配
    fn deserialize_unit_struct<V>(self, name: &'static str, visitor: V) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        if self.next_char()? != b'*' {
            return Err(Error::ExpectedStarSign);
        }
        if let Some(1) = self.next_length_hint()? {
            match self.parse_bulk_string()? {
                Some(parsed_name) => {
                    if parsed_name == name.as_bytes() {
                        // 检查完成，提示 visitor 可以直接构建 unit struct
                        visitor.visit_unit()
                    } else {
                        Err(Error::MismatchedName)
                    }
                }
                None => Err(Error::MismatchedName),
            }
        } else {
            Err(Error::BadLengthHint)
        }
    }

    fn deserialize_newtype_struct<V>(self, name: &'static str, visitor: V) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        if self.next_char()? != b'*' {
            return Err(Error::ExpectedStarSign);
        }
        if let Some(2) = self.next_length_hint()? {
            match self.parse_bulk_string()? {
                Some(parsed_name) => {
                    if parsed_name == name.as_bytes() {
                        // 检查完成，visitor 继续构建 newtype
                        visitor.visit_newtype_struct(self)
                    } else {
                        Err(Error::MismatchedName)
                    }
                }
                None => Err(Error::MismatchedName),
            }
        } else {
            return Err(Error::BadLengthHint);
        }
    }

    fn deserialize_seq<V>(mut self, visitor: V) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        // Parse the opening bracket of the sequence.
        if self.next_char()? == b'*' {
            if let Some(len) = self.next_length_hint()? {
                visitor.visit_seq(BulkStrings::new(&mut self, len as u64))
            } else {
                // null 值已有 null bulk string, 这里默认失败
                Err(Error::ExpectedArray)
            }
        } else {
            Err(Error::ExpectedStarSign)
        }
    }

    fn deserialize_tuple<V>(self, _len: usize, visitor: V) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        self.deserialize_seq(visitor)
    }

    // Tuple structs 消耗第一项来检查name，然后和 seq 解析相同
    fn deserialize_tuple_struct<V>(
        mut self,
        name: &'static str,
        len: usize,
        visitor: V,
    ) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        if self.next_char()? != b'*' {
            return Err(Error::ExpectedStarSign);
        }

        if let Some(parsed_len) = self.next_length_hint()? {
            if parsed_len != len + 1 {
                return Err(Error::MismatchedLengthHint);
            }
            match self.parse_bulk_string()? {
                Some(parsed_name) => {
                    if parsed_name == name.as_bytes() {
                        // 检查完成，visitor 继续构建 newtype
                        visitor.visit_seq(BulkStrings::new(&mut self, len as u64))
                    } else {
                        Err(Error::MismatchedName)
                    }
                }
                None => Err(Error::MismatchedName),
            }
        } else {
            // null 值已有 null bulk string, 这里默认失败
            Err(Error::MismatchedLengthHint)
        }
    }

    // resp 的反序列化暂时都可以通过 visit_seq 实现
    // reser 也不支持 map 类型的序列化
    fn deserialize_map<V>(self, _visitor: V) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        unimplemented!()
    }

    // 标识符，一定是来自与结构体中的字段或者枚举项。所以一定是BulkString中的一项
    fn deserialize_identifier<V>(self, visitor: V) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        self.deserialize_str(visitor)
    }

    fn deserialize_struct<V>(
        self,
        name: &'static str,
        fields: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        // 直接使用 tuple struct， 省略 field name 的匹配检查
        self.deserialize_tuple_struct(name, fields.len(), visitor)
    }

    fn deserialize_enum<V>(
        mut self,
        _name: &'static str,
        _variants: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        // enum 体现为一个 array of bulk string, 所以不用检查name匹配，
        // 到内部 variant 反序列化时处理
        if self.next_char()? != b'*' {
            return Err(Error::ExpectedStarSign);
        }

        if let Some(len) = self.next_length_hint()? {
            visitor.visit_enum(BulkStrings::new(&mut self, len as u64))
        } else {
            // null 值已有 null bulk string, 这里默认失败
            Err(Error::MismatchedLengthHint)
        }
    }
}

struct BulkStrings<'a, 'de: 'a, R> {
    de: &'a mut Deserializer<'de, R>,
    cnt: u64,
}

impl<'a, 'de, R> BulkStrings<'a, 'de, R> {
    fn new(de: &'a mut Deserializer<'de, R>, cnt: u64) -> Self {
        BulkStrings { de, cnt }
    }
}

impl<'a, 'de, R: io::Read> SeqAccess<'de> for BulkStrings<'a, 'de, R> {
    type Error = Error;

    fn next_element_seed<T>(&mut self, seed: T) -> Result<Option<T::Value>>
    where
        T: DeserializeSeed<'de>,
    {
        if self.cnt == 0 {
            return Ok(None);
        }
        self.cnt -= 1;
        seed.deserialize(&mut *self.de).map(Some)
    }
}

// enum的实际项目只有一项，所以 EnumAccess 和 VariantAccess 的方法都传入self
// 仅一次调用
impl<'a, 'de, R: io::Read> EnumAccess<'de> for BulkStrings<'a, 'de, R> {
    type Error = Error;
    type Variant = Self;

    fn variant_seed<V>(mut self, seed: V) -> Result<(V::Value, Self::Variant)>
    where
        V: DeserializeSeed<'de>,
    {
        let val = seed.deserialize(&mut *self.de)?;
        self.cnt -= 1;
        if self.cnt > 0 && self.de.peek_char()? != b'$' {
            return Err(Error::ExpectedMoreBulkString);
        } else {
            Ok((val, self))
        }
    }
}

// 细分枚举项的类型
impl<'a, 'de, R: io::Read> VariantAccess<'de> for BulkStrings<'a, 'de, R> {
    type Error = Error;

    fn unit_variant(self) -> Result<()> {
        if self.cnt == 0 {
            Ok(())
        } else {
            Err(Error::ExpectedDollarSign)
        }
    }

    fn newtype_variant_seed<T>(self, seed: T) -> Result<T::Value>
    where
        T: DeserializeSeed<'de>,
    {
        seed.deserialize(self.de)
    }

    fn tuple_variant<V>(self, _len: usize, visitor: V) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        visitor.visit_seq(self)
    }

    fn struct_variant<V>(self, _fields: &'static [&'static str], visitor: V) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        visitor.visit_seq(self)
    }
}

////////////////////////////////////////////////////////////////////////////////

#[test]
fn test_unit_struct() {
    #[derive(serde::Deserialize, PartialEq, Debug)]
    struct Test;

    let r = b"*1\r\n$4\r\nTest\r\n";
    assert_eq!(Test, from_bytes(r).unwrap());
    let r = b"*1\r\n$3\r\nTst\r\n";
    match from_bytes::<Test>(r) {
        Err(Error::MismatchedName) => assert!(true),
        _ => assert!(false, "MismatchedName error not found"),
    }
}

#[test]
fn test_newtype_struct() {
    #[derive(serde::Deserialize, PartialEq, Debug)]
    struct Test(String);

    let r = b"*2\r\n$4\r\nTest\r\n$4\r\ntest\r\n";
    assert_eq!(Test("test".to_owned()), from_bytes(r).unwrap());
    let r = b"*2\r\n$3\r\nTst\r\n$4\r\ntest\r\n";
    match from_bytes::<Test>(r) {
        Err(Error::MismatchedName) => assert!(true),
        _ => assert!(false, "MismatchedName error not found"),
    }
}

#[test]
fn test_seq() {
    let r = b"*2\r\n$4\r\nTest\r\n$4\r\ntest\r\n";
    let vec_r: Vec<String> = from_bytes(r).unwrap();
    let tuple_r: (String, String) = from_bytes(r).unwrap();
    assert_eq!(vec!["Test".to_owned(), "test".to_owned()], vec_r);
    assert_eq!(("Test".to_owned(), "test".to_owned()), tuple_r);
}

#[test]
fn test_tuple_struct() {
    #[derive(serde::Deserialize, PartialEq, Debug)]
    struct Test(String, String);

    let r = b"*3\r\n$4\r\nTest\r\n$4\r\ntest\r\n$3\r\nnil\r\n";
    assert_eq!(
        Test("test".to_owned(), "nil".to_owned()),
        from_bytes(r).unwrap()
    )
}

#[test]
fn test_struct() {
    #[derive(serde::Deserialize, PartialEq, Debug)]
    struct Test {
        key: String,
        val: u32,
        arr: Vec<u32>,
    }

    let r = b"*4\r\n$4\r\nTest\r\n$1\r\na\r\n$2\r\n42\r\n*3\r\n$1\r\n1\r\n$1\r\n2\r\n$1\r\n3\r\n";
    assert_eq!(
        Test {
            key: "a".to_owned(),
            val: 42,
            arr: vec![1, 2, 3],
        },
        from_bytes(r).unwrap()
    )
}

#[test]
fn test_enum() {
    #[derive(serde::Deserialize, PartialEq, Debug)]
    enum Test {
        Unit,
        Newtype(u32),
        Tuple(u32, u32),
        Struct { a: u32 },
    }

    assert_eq!(Test::Unit, from_bytes(b"*1\r\n$4\r\nUnit\r\n").unwrap());
    assert_eq!(
        Test::Newtype(1),
        from_bytes(b"*2\r\n$7\r\nNewtype\r\n$1\r\n1\r\n").unwrap()
    );
    assert_eq!(
        Test::Tuple(1, 2),
        from_bytes(b"*3\r\n$5\r\nTuple\r\n$1\r\n1\r\n$1\r\n2\r\n").unwrap()
    );
    assert_eq!(
        Test::Struct { a: 1 },
        from_bytes(b"*2\r\n$6\r\nStruct\r\n$1\r\n1\r\n").unwrap()
    );
}
