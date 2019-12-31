use serde_resp::{to_bytes, Error};

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

#[test]
fn test_float_fail() {
    let f = vec![3.2, 1.4];
    match to_bytes(&f) {
        Err(Error::Message(msg)) => assert!(msg.find("support").is_some()),
        _ => assert!(false, "no error when serializing float")
    }
}
