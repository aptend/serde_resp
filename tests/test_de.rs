use serde_resp::de;
use serde_resp::{from_reader, Error};

macro_rules! R {
    ($b: expr) => {
        &$b.to_vec()[..]
    };
}

#[test]
fn test_unit_struct() {
    #[derive(serde::Deserialize, PartialEq, Debug)]
    struct Test;

    let r = R!(b"*1\r\n$4\r\nTest\r\n");
    assert_eq!(Test, from_reader(r).unwrap());
    let r = R!(b"*1\r\n$3\r\nTst\r\n");
    match from_reader::<_, Test>(r) {
        Err(Error::MismatchedName) => assert!(true),
        _ => assert!(false, "MismatchedName error not found"),
    }
}

#[test]
fn test_newtype_struct() {
    #[derive(serde::Deserialize, PartialEq, Debug)]
    struct Test(String);

    let r = R!(b"*2\r\n$4\r\nTest\r\n$4\r\ntest\r\n");
    assert_eq!(Test("test".to_owned()), from_reader(r).unwrap());
    let r = R!(b"*2\r\n$3\r\nTst\r\n$4\r\ntest\r\n");
    match from_reader::<_, Test>(r) {
        Err(Error::MismatchedName) => assert!(true),
        _ => assert!(false, "MismatchedName error not found"),
    }
}

#[test]
fn test_seq() {
    let r = R!(b"*2\r\n$4\r\nTest\r\n$4\r\ntest\r\n");
    let vec_r: Vec<String> = from_reader(r).unwrap();
    let tuple_r: (String, String) = from_reader(r).unwrap();
    assert_eq!(vec!["Test".to_owned(), "test".to_owned()], vec_r);
    assert_eq!(("Test".to_owned(), "test".to_owned()), tuple_r);
}

#[test]
fn test_tuple_struct() {
    #[derive(serde::Deserialize, PartialEq, Debug)]
    struct Test(String, String);

    let r = R!(b"*3\r\n$4\r\nTest\r\n$4\r\ntest\r\n$3\r\nnil\r\n");
    assert_eq!(
        Test("test".to_owned(), "nil".to_owned()),
        from_reader(r).unwrap()
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

    let r =
        R!(b"*4\r\n$4\r\nTest\r\n$1\r\na\r\n$2\r\n42\r\n*3\r\n$1\r\n1\r\n$1\r\n2\r\n$1\r\n3\r\n");
    assert_eq!(
        Test {
            key: "a".to_owned(),
            val: 42,
            arr: vec![1, 2, 3],
        },
        from_reader(r).unwrap()
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

    assert_eq!(
        Test::Unit,
        from_reader(R!(b"*1\r\n$4\r\nUnit\r\n")).unwrap()
    );
    assert_eq!(
        Test::Newtype(1),
        from_reader(R!(b"*2\r\n$7\r\nNewtype\r\n$1\r\n1\r\n")).unwrap()
    );
    assert_eq!(
        Test::Tuple(1, 2),
        from_reader(R!(b"*3\r\n$5\r\nTuple\r\n$1\r\n1\r\n$1\r\n2\r\n")).unwrap()
    );
    assert_eq!(
        Test::Struct { a: 1 },
        from_reader(R!(b"*2\r\n$6\r\nStruct\r\n$1\r\n1\r\n")).unwrap()
    );
}

#[test]
fn test_iter() {
    #[derive(serde::Deserialize, PartialEq, Debug)]
    enum Test {
        Unit,
        Newtype(u32),
        Tuple(u32, u32),
        Struct { a: u32 },
    }
    let bytes = R!(b"*1\r\n$4\r\nUnit\r\n*2\r\n$7\r\nNewtype\r\n$1\r\n1\r\n");
    let mut iter = de::Deserializer::from_reader(bytes).into_iter::<Test>();
    match iter.next() {
        Some(Ok(Test::Unit)) => assert!(true),
        _ => assert!(false, "failed to de Unit"),
    };
    match iter.next() {
        Some(Ok(Test::Newtype(1))) => assert!(true),
        _ => assert!(false, "failed to de Newtype"),
    };
    match iter.next() {
        None => assert!(true),
        _ => assert!(false, "failed to stop iter"),
    };
}

#[test]
fn test_char() {
    assert_eq!(
        from_reader::<_, char>("$4\r\n🌟\r\n".as_bytes()).unwrap(),
        '🌟'
    );
}
