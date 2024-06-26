use hashtable::{HMap, HNode, Link};
use lazy_static::lazy_static;
use mio::net::{TcpListener, TcpStream};
use mio::{Events, Interest, Poll, Token};
use std::cell::RefCell;
use std::collections::HashMap;
use std::io::{self, Error, ErrorKind, Read, Write};
use std::rc::Rc;
use std::sync::Mutex;
use std::time::Duration;
mod hashtable;
const SERVER: Token = Token(0);
const K_MAX_MSG: usize = 4096;
const K_MAX_ARGS: usize = 1024;

#[derive(Debug, PartialEq)]
enum State {
    Reading,
    Writing,
    Closed,
}

lazy_static! {
    static ref G_MAP: Mutex<HashMap<String, String>> = Mutex::new(HashMap::new());
}
fn cmd_is(word: &String, cmd: &str) -> bool {
    word.eq_ignore_ascii_case(cmd)
}

struct Conn {
    state: State,
    stream: TcpStream,
    rbuf_size: usize,
    rbuf: [u8; 4 + K_MAX_MSG],
    wbuf_size: usize,
    wbuf: [u8; 4 + K_MAX_MSG],
    wbuf_sent: usize,
}

impl Conn {
    fn state_req(&mut self) {
        while self.try_fill_buffer() {}
    }

    fn try_fill_buffer(&mut self) -> bool {
        assert!(self.rbuf_size < self.rbuf.len());
        match self.read() {
            Ok(n) => {
                println!(
                    "I just read {n} bytes and am filled with {} bytes of data",
                    self.rbuf_size
                );

                if n == 0 {
                    if self.rbuf_size > 0 {
                        println!("unexpected EOF");
                    } else {
                        println!("EOF");
                    }
                    self.state = State::Closed;
                    return false;
                }
                self.rbuf_size += n;
                assert!(self.rbuf_size <= self.rbuf.len());

                while self.try_one_request() {}
                return self.state == State::Reading;
            }
            Err(ref e) if e.kind() == ErrorKind::WouldBlock => {
                return false;
            }
            Err(ref e) if e.kind() == ErrorKind::Interrupted => {
                return true;
            }
            Err(_) => {
                self.state = State::Closed;
                return false;
            }
        }
    }

    fn try_one_request(&mut self) -> bool {
        if self.rbuf_size < 4 {
            return false;
        }

        let buf = &self.rbuf[..4];
        let len = u32::from_le_bytes([buf[0], buf[1], buf[2], buf[3]]) as usize;
        println!("{} is the length of the message", len);
        if len > K_MAX_MSG {
            println!("len too big: {}", len);
            self.state = State::Closed;
            return false;
        }
        // not enough data in buffer retry again

        if 4 + len > self.rbuf_size {
            return false;
        }
        // got one request, generate the response
        println!("The client says {:?}", &self.rbuf[4..len + 4]);


        let mut cmd :Vec<String>=vec![];
        match self.parse_req(len, &mut cmd) {
            std::io::Result::Ok(_) => {}
            Err(E) => {
                println!("{:?}", E);
                self.state=State::Closed;
                return false;
            }
        }
        let mut out:Vec<u8>=Vec::new();
        println!("Successfully parsed!");
        match self.do_request(&mut cmd,&mut out) {
            std::io::Result::Ok(_) => {}

            Err(_) => {
                self.state = State::Closed;
                return false;
            }
        }

        if 4+out.len()>K_MAX_MSG{
            out.clear();
            self.out_err(&mut out, ErrorCode::RES_NX, &String::from("Response is too big"));
        }



        let wlen=out.len();
        self.wbuf[0..4].copy_from_slice(&(wlen as u32).to_le_bytes());
        self.wbuf[4..4+out.len()].copy_from_slice(&out);

        self.wbuf_size = 4 + wlen;

        println!("{:?}", &self.wbuf[..self.wbuf_size]);

        //removing the request from the buffer

        let remaining = self.rbuf_size - 4 - len;

        if remaining > 0 {
            self.rbuf[..].copy_within((4 + len)..(4 + len + remaining), 0);
        }
        self.rbuf_size = remaining;

        self.state = State::Writing;

        self.state_res();

        return self.state == State::Reading;
    }

    fn do_request(
        &mut self,
        cmd: &mut Vec<String>,
        out: &mut Vec<u8>,
    ) -> std::io::Result<()> {
        
        if(cmd.len()==1 && cmd_is(&cmd[0],"keys" )){
           self.do_keys(cmd,out);
        }
        else if cmd.len() == 2 && cmd_is(&cmd[0], "get") {
            self.do_get(cmd, out);
        } else if cmd.len() == 3 && cmd_is(&cmd[0], "set") {
             self.do_set(cmd, out);
        } else if cmd.len() == 2 && cmd_is(&cmd[0], "del") {
            self.do_del(cmd, out);
        } else {
            self.out_err(out,ErrorCode::RES_ERR,&String::from("Unknown CMD"));
        }

        std::io::Result::Ok(())
    }
    fn do_keys(&mut self,cmd:&Vec<String>,out: &mut Vec<u8>){
        let map=G_MAP.lock().unwrap();
        let len=map.len();
        let keys=map.keys();
        self.out_arr(out,len);
        for key in keys{
            self.out_str(out,key);
        }

    }
    fn out_arr(&mut self,out: &mut Vec<u8>,len:usize){
        out.push(Serialization::SER_ARR as u8);
        out.extend((len as u32).to_le_bytes());
    }
    fn out_err(&mut self,out:&mut Vec<u8>,err:ErrorCode,msg:&String){
        out.push(Serialization::SER_ERR as u8);
        out.extend((err as u32).to_le_bytes());
        let len=msg.as_bytes().len();
        out.extend((len as u32).to_le_bytes());
        out.extend(msg.as_bytes());

    }
    fn out_int(&mut self,out: &mut Vec<u8>,n:u32){
        out.push(Serialization::SER_INT as u8);
        out.extend((n).to_le_bytes());
    }
    fn out_str(&mut self,out: &mut Vec<u8>,s:&String){
        out.push(Serialization::SER_STR as u8);
        let len=s.as_bytes().len();
        out.extend((len as u32).to_le_bytes());
        out.extend(s.as_bytes());
    }
    fn out_nil(&mut self,out: &mut Vec<u8>){
        out.push(Serialization::SER_NIL as u8);
    }

























    fn parse_req(&mut self, reqlen: usize, cmd: &mut Vec<String>) -> std::io::Result<()> {
        if reqlen < 8 {
            return Err(Error::new(io::ErrorKind::Other, "Bad request!"));
        }

        // Extract the number of commands from bytes 4 to 8
        let mut zN = [0_u8; 4];
        zN.copy_from_slice(&self.rbuf[4..8]);
        let mut n = u32::from_le_bytes(zN);

        println!("{} number of strings", n);
        if n > K_MAX_ARGS as u32 {
            return Err(Error::new(io::ErrorKind::Other, "n > K_MAX_ARGS"));
        }

        let mut pos = 8_usize; // Start reading commands from position 8
        while n > 0 {
            n -= 1;
            println!("{pos}");
            // Check if there are enough bytes to read the length of the command
            if pos > reqlen {
                return Err(Error::new(io::ErrorKind::Other, "string not expected"));
            }

            let mut zs = [0_u8; 4];
            zs.copy_from_slice(&self.rbuf[pos..pos + 4]);
            let sz = u32::from_le_bytes(zs);
            println!("sz is {sz}");

            // Check if there are enough bytes to read the command content
            if pos + (sz as usize) > reqlen {
                return Err(Error::new(io::ErrorKind::Other, "too less information"));
            }

            let message = String::from_utf8_lossy(&self.rbuf[pos + 4..pos + 4 + (sz as usize)]);
            cmd.push(message.to_string());
            println!("the command is {:?}", cmd);

            pos += 4 + (sz as usize);
        }

        if pos != reqlen + 4 {
            return Err(Error::new(io::ErrorKind::InvalidData, "Garbage trailing!"));
        }

        Ok(())
    }
    fn do_get(&mut self, cmd: &Vec<String>, out: &mut Vec<u8>)  {
        let map = G_MAP.lock().unwrap();

        
        let val = map.get(&cmd[1]);
        if val.is_none() {
            self.out_nil(out);
            return;
        }
        let val = val.unwrap();
        self.out_str(out,val);
        return;
    }

    fn do_set(&mut self, cmd: &Vec<String>, out: &mut Vec<u8>)  {
        let mut map = G_MAP.lock().unwrap();
        map.insert(cmd[1].clone(), cmd[2].clone());
        self.out_nil(out);
        return;
    }

    fn do_del(&mut self, cmd: &Vec<String>, out: &mut Vec<u8>)  {
        let mut map = G_MAP.lock().unwrap();
        let v=map.remove(&cmd[1]);
        if v.is_none() {
            self.out_int(out,0);
        }
        else{
            self.out_int(out,1);
        }
        return;
    }

    fn state_res(&mut self) {
        while self.try_flush_buffer() {}
    }
    fn try_flush_buffer(&mut self) -> bool {
        assert!(self.rbuf_size < self.rbuf.len());
        match self.write() {
            std::io::Result::Ok(n) => {
                self.wbuf_sent += n;
                assert!(self.wbuf_sent <= self.wbuf_size);
                if self.wbuf_sent == self.wbuf_size {
                    self.wbuf_sent = 0;
                    self.wbuf_size = 0;
                    self.state = State::Reading;
                    return false;
                }
                return true;
            }
            Err(ref e) if e.kind() == ErrorKind::WouldBlock => {
                return false;
            }
            Err(ref e) if e.kind() == ErrorKind::Interrupted => {
                return true;
            }
            Err(_) => {
                self.state = State::Closed;
                return false;
            }
        }
    }

    fn connection_io(&mut self) {
        if self.state == State::Reading {
            self.state_req();
            println!("Socket in reading state");
        } else if self.state == State::Writing {
            self.state_res();
            println!("Socket in writing state");
        }
    }

    fn new(stream: TcpStream) -> Self {
        Conn {
            state: State::Reading,
            stream,
            rbuf_size: 0,
            rbuf: [0; 4 + K_MAX_MSG],
            wbuf_size: 0,
            wbuf: [0; 4 + K_MAX_MSG],
            wbuf_sent: 0,
        }
    }

    fn read(&mut self) -> std::io::Result<usize> {
        match self.stream.read(&mut self.rbuf[self.rbuf_size..]) {
            Ok(n) => Ok(n),
            Err(e) => Err(e),
        }
    }

    fn write(&mut self) -> std::io::Result<usize> {
        if self.wbuf_sent < self.wbuf_size {
            match self
                .stream
                .write(&self.wbuf[self.wbuf_sent..self.wbuf_size])
            {
                Ok(n) => Ok(n),
                Err(e) => Err(e),
            }
        } else {
            Ok(0)
        }
    }

    fn close(&mut self) {
        self.state = State::Closed;
        let _ = self.stream.shutdown(std::net::Shutdown::Both);
    }
}

impl Drop for Conn {
    fn drop(&mut self) {
        println!("Dropping connection and closing stream.");
        self.close();
    }
}
#[repr(u32)]
enum ErrorCode {
    RES_OK = 0,
    RES_ERR = 1,
    RES_NX = 2,
}

enum Serialization{
    SER_NIL = 0,    // Like `NULL`
    SER_ERR = 1,    // An error code and message
    SER_STR = 2,    // A string
    SER_INT = 3,    // A int64
    SER_ARR = 4,    // Array
}








fn main() -> std::io::Result<()> {
    let addr = "127.0.0.1:8080".parse().unwrap();
    let mut listener = TcpListener::bind(addr)?;

    // Create a Poll instance
    let mut poll = Poll::new()?;

    // Create storage for events
    let mut events = Events::with_capacity(128);

    // Register the listener with Poll
    poll.registry()
        .register(&mut listener, SERVER, Interest::READABLE)?;

    // A map of all client connections, keyed by Token
    let mut connections: HashMap<Token, Conn> = HashMap::new();
    let mut next_token = Token(SERVER.0 + 1);

    loop {
        // Poll for events with a timeout
        poll.poll(&mut events, Some(Duration::from_millis(1000)))?;

        for event in events.iter() {
            match event.token() {
                SERVER => {
                    // Accept new connections

                    match listener.accept() {
                        Ok((stream, _)) => {
                            let token = next_token;
                            next_token.0 += 1;

                            // Create a new connection
                            let mut conn = Conn::new(stream);

                            // Register the new connection
                            poll.registry().register(
                                &mut conn.stream,
                                token,
                                Interest::READABLE | Interest::WRITABLE,
                            )?;

                            connections.insert(token, conn);
                            println!("New connection created");
                        }
                        Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {}
                        Err(e) => return Err(e),
                    }
                }
                token => {
                    // Handle client connections
                    if let Some(conn) = connections.get_mut(&token) {
                        conn.connection_io();
                        if matches!(conn.state, State::Closed) {
                            connections.remove(&token);
                        }
                    }
                }
            }
        }
    }
}
