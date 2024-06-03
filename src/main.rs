use mio::net::{TcpListener, TcpStream};
use mio::{Events, Interest, Poll, Token};
use std::collections::HashMap;
use std::fs::copy;
use std::io::{ErrorKind, Read, Write};
use std::path::Component;
use std::time::Duration;

const SERVER: Token = Token(0);
const K_MAX_MSG: usize = 4096;

#[derive(Debug,PartialEq)]
enum State {
    Reading,
    Writing,
    Closed,
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
    fn state_req(&mut self){
        while self.try_fill_buffer(){}

    }

    fn try_fill_buffer(&mut self)->bool{
        assert!(self.rbuf_size<self.rbuf.len());
        match self.read(){
            Ok(n) =>{       
             println!("I just read {n} bytes and am filled with {} bytes of data",self.rbuf_size);

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
                assert!(self.rbuf_size<=self.rbuf.len());

                while self.try_one_request() {}
                return self.state==State::Reading;



            },
            Err(ref e) if e.kind()==ErrorKind::WouldBlock =>{
                return false;
            }
            Err(ref e) if e.kind()==ErrorKind::Interrupted=>{
                return true;
            }
            Err(_)=>{
                self.state=State::Closed;
                return false;
            }
        }
       
    }


    fn try_one_request(&mut self)->bool{
        if self.rbuf_size < 4 {
            return false;
        }

        let mut len=0;
        let buf = &self.rbuf[..4];
        let len=u32::from_le_bytes([buf[0],buf[1],buf[2],buf[3]]) as usize;

        if len>K_MAX_MSG{
            println!("len too big: {}",len);
            self.state=State::Closed;
            return false;
        }

        if 4+len>self.rbuf_size{
            return false;

        }

        println!("The client says {}",String::from_utf8_lossy(&self.rbuf[4..len+4]) );

        self.wbuf[0..4].copy_from_slice(&self.rbuf[..4]);

        self.wbuf[4..4+len].copy_from_slice(&self.rbuf[4..4+len]);

        self.wbuf_size=4+len;

        let remaining=self.rbuf_size-4-len;
    
        if remaining>0{
            self.rbuf[..].copy_within(4 + len .. 4 + len + remaining, 0);
        }
        self.rbuf_size=remaining;

        self.state=State::Writing;

        self.state_res();


        return self.state==State::Reading;



    }

    fn state_res(&mut self){
        while self.try_flush_buffer(){}
    }
    fn try_flush_buffer(&mut self)->bool{
        assert!(self.rbuf_size<self.rbuf.len());
        match self.write(){
            Ok(n) =>{
            
                    self.wbuf_sent += n;
                    assert!(self.wbuf_sent <= self.wbuf_size);
                    if self.wbuf_sent == self.wbuf_size {
                        self.wbuf_sent = 0;
                        self.wbuf_size = 0;
                        self.state = State::Reading;
                        return false;
                    }
                    return true;
                    
             
               


            },
            Err(ref e) if e.kind()==ErrorKind::WouldBlock =>{
                return false;
            }
            Err(ref e) if e.kind()==ErrorKind::Interrupted=>{
                return true;
            }
            Err(_)=>{
                self.state=State::Closed;
                return false;
            }
        }

    }

    fn connection_io(&mut self){
        if self.state==State::Reading{
            self.state_req();
            println!("Socket in reading state");
        }
        else if self.state==State::Writing {
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
            Ok(n) => {
                
                Ok(n)
            },
            Err(e) => Err(e),
        }
    }

    fn write(&mut self) -> std::io::Result<usize> {
        if self.wbuf_sent < self.wbuf_size {
            match self.stream.write(&self.wbuf[self.wbuf_sent..self.wbuf_size]) {
                Ok(n) => {
                    
                    Ok(n)
                },
                Err(e) => Err(e),
            }
        } else {
            Ok(0)
        }
    }

    fn prepare_write(&mut self, data: &[u8]) -> std::io::Result<()> {
        let len = data.len();
        if len > K_MAX_MSG {
            return Err(std::io::Error::new(std::io::ErrorKind::InvalidInput, "message too long"));
        }
        self.wbuf[0..4].copy_from_slice(&(len as u32).to_le_bytes());
        self.wbuf[4..4 + len].copy_from_slice(data);
        self.wbuf_size = 4 + len;
        self.wbuf_sent = 0;
        self.state = State::Writing;
        Ok(())
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

fn main() -> std::io::Result<()> {
    let addr = "127.0.0.1:8080".parse().unwrap();
    let mut listener = TcpListener::bind(addr)?;

    // Create a Poll instance
    let mut poll = Poll::new()?;

    // Create storage for events
    let mut events = Events::with_capacity(128);

    // Register the listener with Poll
    poll.registry().register(&mut listener, SERVER, Interest::READABLE)?;

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
                            Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {},
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
