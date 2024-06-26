use std::io::{self, Error, ErrorKind, Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::convert::TryInto;
use std::ffi::CString;
use std::vec;

fn msg(msg: &str) {
    eprintln!("{}", msg);
}

fn die(msg: &str) -> ! {
    let err = io::Error::last_os_error();
    eprintln!("[{}] {}", err.raw_os_error().unwrap_or(0), msg);
    std::process::abort();
}

fn read_full(fd: &mut TcpStream, buf: &mut [u8]) -> io::Result<()> {
    let mut total_read = 0;
    while total_read < buf.len() {
        match fd.read(&mut buf[total_read..]) {
            Ok(0) => {
                return Err(Error::new(ErrorKind::UnexpectedEof, "unexpected EOF"));
            }
            Ok(n) => {
                total_read += n;
            }
            Err(ref e) if e.kind() == ErrorKind::Interrupted => {}
            Err(e) => {
                return Err(e);
            }
        }
    }
    Ok(())
}

fn write_all(fd: &mut TcpStream, buf: &[u8]) -> io::Result<()> {
    let mut total_written = 0;
    while total_written < buf.len() {
        match fd.write(&buf[total_written..]) {
            Ok(0) => {
                return Err(Error::new(ErrorKind::WriteZero, "write zero bytes"));
            }
            Ok(n) => {
                total_written += n;
            }
            Err(ref e) if e.kind() == ErrorKind::Interrupted => {}
            Err(e) => {
                return Err(e);
            }
        }
    }
    Ok(())
}

const K_MAX_MSG: usize = 4096;

fn send_req(fd: &mut TcpStream, cmd: &Vec<String>) -> io::Result<()> {
    let mut len = 4u32;
    for s in cmd {
        len += 4 + s.len() as u32;
    }
    if len > K_MAX_MSG as u32 {
        return Err(Error::new(ErrorKind::InvalidInput, "message too large"));
    }

    let mut wbuf = vec![0; 4 + K_MAX_MSG];
    wbuf[0..4].copy_from_slice(&len.to_le_bytes());
    let n = cmd.len() as u32;
    wbuf[4..8].copy_from_slice(&n.to_le_bytes());
    let mut cur = 8usize;
    for s in cmd {
        let p = s.len() as u32;
        wbuf[cur..cur + 4].copy_from_slice(&p.to_le_bytes());
        wbuf[cur + 4..cur + 4 + s.len()].copy_from_slice(s.as_bytes());
        cur += 4 + s.len();
    }

    write_all(fd, &wbuf[0..(4 + len) as usize])?;
    Ok(())
}

enum Serialization {
    SER_NIL = 0,
    SER_ERR = 1,
    SER_STR = 2,
    SER_INT = 3,
    SER_ARR = 4,
}

fn on_response(data: &[u8]) -> io::Result<usize> {
    if data.is_empty() {
        msg("bad response");
        return Err(Error::new(ErrorKind::InvalidData, "bad response"));
    }

    match data[0] {
        0 => {
            println!("(nil)");
            Ok(1)
        }
        1 => {
            if data.len() < 1 + 8 {
                msg("bad response");
                return Err(Error::new(ErrorKind::InvalidData, "bad response"));
            }
            let mut code_bytes = [0; 4];
            let mut len_bytes = [0; 4];
            code_bytes.copy_from_slice(&data[1..5]);
            len_bytes.copy_from_slice(&data[5..9]);
            let code = i32::from_le_bytes(code_bytes);
            let len = u32::from_le_bytes(len_bytes) as usize;
            if data.len() < 1 + 8 + len {
                msg("bad response");
                return Err(Error::new(ErrorKind::InvalidData, "bad response"));
            }
            let msg = CString::new(&data[9..9 + len]).expect("CString::new failed");
            println!("(err) {} {}", code, msg.to_str().unwrap());
            Ok(1 + 8 + len)
        }
        2 => {
            if data.len() < 1 + 4 {
                msg("bad response");
                return Err(Error::new(ErrorKind::InvalidData, "bad response"));
            }
            let mut len_bytes = [0; 4];
            len_bytes.copy_from_slice(&data[1..5]);
            let len = u32::from_le_bytes(len_bytes) as usize;
            if data.len() < 1 + 4 + len {
                msg("bad response");
                return Err(Error::new(ErrorKind::InvalidData, "bad response"));
            }
            let msg = CString::new(&data[5..5 + len]).expect("CString::new failed");
            println!("(str) {}", msg.to_str().unwrap());
            Ok(1 + 4 + len)
        }
        3 => {
            if data.len() < 1 + 8 {
                msg("bad response");
                return Err(Error::new(ErrorKind::InvalidData, "bad response"));
            }
            let mut val_bytes = [0; 8];
            val_bytes.copy_from_slice(&data[1..9]);
            let val = i64::from_le_bytes(val_bytes);
            println!("(int) {}", val);
            Ok(1 + 8)
        }
        4 => {
            if data.len() < 1 + 4 {
                msg("bad response");
                return Err(Error::new(ErrorKind::InvalidData, "bad response"));
            }
            let mut len_bytes = [0; 4];
            len_bytes.copy_from_slice(&data[1..5]);
            let len = u32::from_le_bytes(len_bytes) as usize;
            println!("(arr) len={}", len);
            let mut arr_bytes = 1 + 4;
            for _ in 0..len {
                let rv = on_response(&data[arr_bytes..])?;
                if rv == 0 {
                    return Ok(0);
                }
                arr_bytes += rv;
            }
            println!("(arr) end");
            Ok(arr_bytes)
        }
        _ => {
            msg("bad response");
            Err(Error::new(ErrorKind::InvalidData, "bad response"))
        }
    }

}

fn read_res(fd: &mut TcpStream) -> io::Result<()> {
    let mut rbuf = vec![0; 4 + K_MAX_MSG + 1];
    read_full(fd, &mut rbuf[0..4])?;
    let len = u32::from_le_bytes(rbuf[0..4].try_into().unwrap()) as usize;
    if len > K_MAX_MSG {
        msg("too long");
        return Err(Error::new(ErrorKind::InvalidData, "too long"));
    }
    read_full(fd, &mut rbuf[4..(4 + len)])?;
    on_response(&rbuf[4..(4 + len)])?;
    Ok(())
}

fn main() -> io::Result<()> {
    let mut args: Vec<String> = std::env::args().collect();
    args.remove(0); // Remove the first argument (program name)

    let addr = SocketAddr::from(([127, 0, 0, 1], 8080));
    let mut stream = TcpStream::connect(addr)?;
    
    send_req(&mut stream, &args)?;
    read_res(&mut stream)?;

    Ok(())
}
