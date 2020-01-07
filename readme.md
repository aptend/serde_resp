**serde for redis simple protocol， 学习用**

### 基本情况:

根据[resp的文档](https://redis.io/topics/protocol)，正常情况下，客户端 --> 服务端， 得用 **Array of Bulk Strings** 的格式告知 **命令及参数**；但是服务端反馈结果给客户端时，可以使用**任意的格式组合**。这其实表示了有两套序列化和反序列化的接口：

1. 客户端的序列化，服务端的反序列化，以 **Array of Bulk Strings** 为中介。该模式下即使是数字，也会先格式化为字符串，比如`$2\r\n42\r\n`
2. 服务端的序列化，客户端的反序列化，以 **任意的格式组合** 为中介。数字直接用`:42\r\n`

目前只实现了第一种，姑且认为服务端和客户端使用的是同一套数据结构，它给客户端发送的也是**Array of Bulk Strings**，实际类型由两端公认的数据结构进行约束。第二种应该由serde_resp提供一个枚举类型作中转。


### Serializer主要特点:

- 不支持浮点数、HashMap
- `unit_struct`，形如`struct Foo;`，看成无参数命令
- `newtype_struct`，形如`struct Foo(i32);`，单一参数命令
- `tuple_struct`，形如`struct Foo(i32, i32, i32);`，多参数命令，总长度为参数长度+1
- `struct`，形如`struct Foo {key: i32, val:i32}`，多参数命令，同`tuple_struct`
- 枚举的variant基本上延续和struct相同处理方式

### Deserializer主要特点:

- 不支持浮点数、HashMap
- 构造函数`from_reader`，目标类型仅支持`DeserializeOwned`
- 自造的parser，有较大的提升空间
- 提供`into_iter`，支持pipeline命令解析


### Examples:

```rust
#[derive(Debug, Serialize, Deserialize)]
enum Request {
    Get { key: String },
    Set { key: String, value: String },
    Remove { key: String },
}


fn main() {
    // serialization
    let get = Request::Get { key: "key1".to_owned() };
    assert_eq!(to_bytes(&get), b"*2\r\n$3\r\nGet\r\n$3\r\key1\r\n");
    let set = Request::Set { key: "key1".to_owned(), value: "value1".to_owned() };
    assert_eq!(to_bytes(&set), b"*3\r\n$3\r\nSet\r\n$3\r\key1\r\n$5\r\value1\r\n");
    let rm = Request::Remove { key: "key1".to_owned() };
    assert_eq!(to_bytes(&rm), b"*2\r\n$5\r\nRemove\r\n$3\r\key1\r\n");


    // deserailization
    let get = "*2\r\n$3\r\nGet\r\n$3\r\key1\r\n".as_bytes();
    match from_reader::<_, Request>(get).unwrap() {
        Request::Get{key} => assert_eq!(key, "key1".to_owned()),
        _ => assert!(false, "fail to deserialize into `Get`")
    }
    
    let get = "*3\r\n$3\r\nSet\r\n$3\r\key1\r\n$5\r\value1\r\n".as_bytes();
    match from_reader::<_, Request>(get).unwrap() {
        Request::Set{key, val} => {
            assert_eq!(key, "key1".to_owned());
            assert_eq!(value, "value1".to_owned());
        },
        _ => assert!(false, "fail to deserialize into `Set`")
    }
}
```
