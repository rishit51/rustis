

struct Hnode{
    next:Option<Box<Hnode>>,
    h_code:u64
}


struct Htab{
    tab: Vec<Box<Hnode>>,
    mask:usize,
    size:usize
}



impl Htab{
    fn new()->Sek{}
}