use std::fmt;
use crate::{CompoundTag,ListTag,NbtString,Tag};

/// Brigadier-compatible raw message and UTF-16 cursor. Translation remains the command layer's job.
#[derive(Clone,Debug,PartialEq,Eq)]
pub struct ParseError {pub cursor:usize,pub message:String}
impl fmt::Display for ParseError {fn fmt(&self,f:&mut fmt::Formatter<'_>)->fmt::Result {write!(f,"{} at position {}",self.message,self.cursor)}}
impl std::error::Error for ParseError {}

/// Port of `TagParser#parseFully` using the 26.3 `SnbtGrammar`.
pub fn parse(input:&str)->Result<Tag,ParseError> {let (tag,cursor)=parse_prefix(input)?;let mut p=Parser::at(input,cursor);p.ws();if p.pos<p.text.len() {Err(p.error("Unexpected trailing data"))}else {Ok(tag)}}
pub fn parse_compound(input:&str)->Result<CompoundTag,ParseError> {let tag=parse(input)?;if let Tag::Compound(c)=tag {Ok(c)}else {Err(ParseError{cursor:input.encode_utf16().count(),message:"Expected compound tag".into()})}}
pub fn parse_prefix(input:&str)->Result<(Tag,usize),ParseError> {let mut p=Parser::at(input,0);let tag=p.literal().ok_or_else(||p.best.clone().unwrap_or_else(||p.error("Expected a valid unquoted string")))?;Ok((tag,p.pos))}

pub(crate) struct Parser {text:Vec<u16>,pub(crate) pos:usize,best:Option<ParseError>,depth:usize}
#[derive(Clone)]
struct Integer {negative:bool,radix:u32,digits:String,signed:Option<bool>,kind:Option<char>}
impl Parser {
    pub(crate) fn at(input:&str,pos:usize)->Self {Self{text:input.encode_utf16().collect(),pos,best:None,depth:0}}
    fn error(&self,message:impl Into<String>)->ParseError {ParseError{cursor:self.pos,message:message.into()}}
    fn record<T>(&mut self,cursor:usize,message:impl Into<String>)->Option<T> {if self.best.as_ref().is_none_or(|e|cursor>e.cursor) {self.best=Some(ParseError{cursor,message:message.into()});}None}
    fn fail<T>(&mut self,message:impl Into<String>)->Option<T> {self.record(self.pos,message)}
    fn attempt<T>(&mut self,f:impl FnOnce(&mut Self)->Option<T>)->Option<T> {let pos=self.pos;let result=f(self);if result.is_none() {self.pos=pos;}result}
    fn ws(&mut self) {while self.peek().is_some_and(java_whitespace) {self.pos+=1;}}
    fn peek(&self)->Option<u16> {self.text.get(self.pos).copied()}
    fn chars(&mut self,chars:&str)->Option<char> {self.ws();let start=self.pos;if let Some(c)=self.peek() {self.pos+=1;if chars.chars().any(|v|v as u16==c) {return char::from_u32(u32::from(c));}}self.record(start,format!("Expected literal {}",chars.chars().map(|c|c.to_string()).collect::<Vec<_>>().join("|")))}
    fn ch(&mut self,c:char)->Option<()> {self.chars(&c.to_string()).map(|_|())}
    fn optional_char(&mut self,c:char)->bool {self.attempt(|p|p.ch(c)).is_some()}
    fn sign(&mut self)->bool {if self.attempt(|p|p.ch('+')).is_some() {false} else {self.attempt(|p|p.ch('-')).is_some()}}
    fn run(&mut self,radix:u32)->Option<String> {
        self.ws();let start=self.pos;while self.peek().is_some_and(|c|c==95||char::from_u32(u32::from(c)).is_some_and(|c|c.is_digit(radix))) {self.pos+=1;}
        let end=self.pos;if end==start {return self.fail(match radix {2=>"Expected a binary numeral",16=>"Expected a hexadecimal numeral",_=>"Expected a decimal numeral"})}
        if self.text[start]==95||self.text[end-1]==95 {self.pos=start;return self.fail("Underscores are not allowed at the start or end of a number")}
        Some(String::from_utf16_lossy(&self.text[start..end]))
    }
    fn suffix(&mut self)->Option<(Option<bool>,char)> {
        for (prefix,signed) in [("uU",false),("sS",true)] {if let Some(v)=self.attempt(|p| {p.chars(prefix)?;let kind=p.type_suffix()?;Some((Some(signed),kind))}) {return Some(v)}}
        self.type_suffix().map(|c|(None,c))
    }
    fn type_suffix(&mut self)->Option<char> {for chars in ["bB","sS","iI","lL"] {if let Some(c)=self.attempt(|p|p.chars(chars)) {return Some(c.to_ascii_lowercase())}}None}
    fn integer(&mut self)->Option<Integer> {
        let negative=self.sign();let mut radix=10;
        let digits=if self.attempt(|p|p.ch('0')).is_some() {
            if self.attempt(|p|p.chars("xX")).is_some() {radix=16;self.run(16)?}
            else if let Some(binary)=self.attempt(|p| {p.chars("bB")?;p.run(2)}) {radix=2;binary}
            else if self.attempt(|p|p.run(10)).is_some() {return self.fail("Leading zeroes are not allowed")}
            else {"0".into()}
        } else {self.run(10)?};
        let (signed,kind)=self.attempt(Self::suffix).map(|(s,k)|(s,Some(k))).unwrap_or((None,None));
        Some(Integer{negative,radix,digits,signed,kind})
    }
    fn number(&mut self,n:&Integer,default:char)->Option<Tag> {
        let kind=n.kind.unwrap_or(default);let bits=match kind {'b'=>8,'s'=>16,'i'=>32,'l'=>64,_=>return self.fail("Expected an integer type")};
        let signed=n.signed.unwrap_or(n.radix==10);
        if !signed&&n.negative {return self.fail("Expected a non-negative number")}
        let digits=n.digits.replace('_',"");let fixed=if n.negative {format!("-{digits}")} else {digits.clone()};
        let parsed=u64::from_str_radix(&digits,n.radix).ok().filter(|v| if signed {if n.negative {*v<=1u64<<(bits-1)}else {*v<(1u64<<(bits-1))}}else {bits==64||*v<(1u64<<bits)});
        let Some(v)=parsed else {
            let detail=if signed&&bits<=16 {format!("Value out of range. Value:\"{fixed}\" Radix:{}",n.radix)}
                else if !signed&&bits==8 {format!("out of range: {fixed}")}
                else if !signed&&bits==16 {format!("out of range: {}",u64::from_str_radix(&digits,n.radix).unwrap_or(0))}
                else if !signed&&bits>=32 {format!("String value {fixed} exceeds range of unsigned {}.",if bits==32 {"int"}else {"long"})}
                else {format!("For input string: \"{fixed}\"{}",if n.radix==10 {String::new()}else {format!(" under radix {}",n.radix)})};
            return self.fail(format!("Failed to parse number: {detail}"))
        };
        let value=if n.negative {(v as i64).wrapping_neg()}else {v as i64};Some(match kind {'b'=>Tag::Byte(value as i8),'s'=>Tag::Short(value as i16),'l'=>Tag::Long(value),_=>Tag::Int(value as i32)})
    }
    fn exponent(&mut self)->Option<String> {self.chars("eE")?;let minus=self.sign();let digits=self.run(10)?;Some(format!("e{}{digits}",if minus {"-"}else {""}))}
    fn float_suffix(&mut self)->Option<char> {self.attempt(|p|p.chars("fF")).or_else(||self.chars("dD")).map(|c|c.to_ascii_lowercase())}
    fn float(&mut self)->Option<Tag> {
        let negative=self.sign();
        for branch in 0..4 {
            let result=self.attempt(|p| {
                let (whole,fraction)=match branch {
                    0=>{let w=p.run(10)?;p.ch('.')?;let f=p.attempt(|p|p.run(10));(w,f)},
                    1=>{p.ch('.')?;(String::new(),Some(p.run(10)?))},
                    _=>(p.run(10)?,None),
                };
                let exponent=if branch==2 {Some(p.exponent()?)} else {p.attempt(Self::exponent)};
                let suffix=if branch==3 {Some(p.float_suffix()?)} else {p.attempt(Self::float_suffix)};
                let mut value=if negative {format!("-{whole}")} else {whole};if let Some(fraction)=fraction {value.push('.');value.push_str(&fraction)}if let Some(exponent)=exponent {value.push_str(&exponent)}
                let value=value.replace('_',"");
                if suffix==Some('f') {let v=value.parse::<f32>().ok()?;if !v.is_finite() {return p.fail("Infinity is not allowed")}Some(Tag::Float(if v==0.0 {0.0}else {v}))}
                else {let v=value.parse::<f64>().ok()?;if !v.is_finite() {return p.fail("Infinity is not allowed")}Some(Tag::Double(if v==0.0 {0.0}else {v}))}
            });if result.is_some() {return result}
        }None
    }
    fn unquoted(&mut self)->Option<NbtString> {self.ws();let start=self.pos;while self.peek().is_some_and(|c|matches!(c,48..=57|65..=90|97..=122|95|45|46|43)) {self.pos+=1;}if self.pos==start {self.fail("Expected a valid unquoted string")}else {Some(NbtString::from_utf16(self.text[start..self.pos].to_vec()))}}
    fn quoted(&mut self)->Option<NbtString> {
        let quote=self.chars("\"'")? as u16;let mut out=Vec::new();
        loop {
            let start=self.pos;
            while self.peek().is_some_and(|c|c!=34&&c!=39&&c!=92) {out.push(self.text[self.pos]);self.pos+=1;}
            if self.peek()==Some(quote) {self.pos+=1;return Some(NbtString::from_utf16(out))}
            if self.peek().is_none() {return self.fail("Invalid string contents")}
            if self.peek()==Some(92) {self.pos+=1;self.ws();let escape=self.peek()?;self.pos+=1;
                match escape {
                    98=>out.push(8),115=>out.push(32),116=>out.push(9),110=>out.push(10),102=>out.push(12),114=>out.push(13),92|39|34=>out.push(escape),
                    120|117|85=>{
                        let size=match escape {120=>2,117=>4,_=>8};let start=self.pos;let mut code=0u32;
                        for _ in 0..size {let c=self.peek().and_then(|c|char::from_u32(u32::from(c))).and_then(|c|c.to_digit(16));let Some(c)=c else {return self.record(start,format!("Expected a character literal of length {size}"))};code=(code<<4)|c;self.pos+=1;}
                        if code>0x10ffff {return self.fail(format!("Invalid Unicode character value: U+{code:08X}"))}
                        if code<=65535 {out.push(code as u16)}else {let c=code-0x10000;out.push((0xd800+(c>>10)) as u16);out.push((0xdc00+(c&1023)) as u16);}
                    },
                    78=>{self.ch('{')?;let start=self.pos;while self.peek().is_some_and(|c|matches!(c,45|32|48..=57|65..=90|97..=122)) {self.pos+=1;}let name=String::from_utf16_lossy(&self.text[start..self.pos]);self.ch('}')?;
                        let Some(c)=unicode_names2::character(&name) else {return self.fail("Invalid Unicode character name")};out.extend(c.encode_utf16(&mut [0;2]).iter().copied());},
                    _=>return self.record(self.pos-1,"Expected literal b"),
                }
            }else {out.push(self.text[self.pos]);self.pos+=1;}
            if self.pos==start {return None}
        }
    }
    fn key(&mut self)->Option<NbtString> {self.attempt(Self::quoted).or_else(||self.unquoted())}
    fn map_entry(&mut self)->Option<(NbtString,Tag)> {let key=self.key()?;self.ch(':')?;let value=self.literal()?;if key.is_empty() {return self.fail("Empty keys are not allowed")}Some((key,value))}
    fn compound(&mut self)->Option<Tag> {
        self.ch('{')?;let mut c=CompoundTag::new();
        if let Some((k,v))=self.attempt(Self::map_entry) {c.insert(k,v);while self.optional_char(',') {if let Some((k,v))=self.attempt(Self::map_entry) {c.insert(k,v);}else {break}}}
        self.ch('}')?;Some(Tag::Compound(c))
    }
    fn array(&mut self)->Option<Tag> {
        let kind=self.attempt(|p|p.ch('B').map(|_|'b')).or_else(||self.attempt(|p|p.ch('L').map(|_|'l'))).or_else(||self.ch('I').map(|_|'i'))?;
        self.ch(';')?;let mut entries=Vec::new();if let Some(n)=self.attempt(Self::integer) {entries.push(n);while self.optional_char(',') {if let Some(n)=self.attempt(Self::integer) {entries.push(n)}else {break}}}
        self.ch(']')?;
        let mut tags=Vec::new();for n in entries {if n.kind.is_some_and(|k|match kind {'b'=>k!='b','i'=>!matches!(k,'b'|'s'|'i'),_=>!matches!(k,'b'|'s'|'i'|'l')}) {return self.fail("Invalid array element type")}tags.push(self.number(&n,kind)?);}
        Some(match kind {'b'=>Tag::ByteArray(tags.iter().map(|t|t.as_byte().unwrap()).collect()),'i'=>Tag::IntArray(tags.iter().map(|t|t.as_int().unwrap()).collect()),_=>Tag::LongArray(tags.iter().map(|t|t.as_long().unwrap()).collect())})
    }
    fn list(&mut self)->Option<Tag> {
        self.ch('[')?;if let Some(array)=self.attempt(Self::array) {return Some(array)}
        let mut v=ListTag::default();if let Some(tag)=self.attempt(Self::literal) {v.push(tag);while self.optional_char(',') {if let Some(tag)=self.attempt(Self::literal) {v.push(tag)}else {break}}}self.ch(']')?;Some(Tag::List(v))
    }
    fn builtin(&mut self)->Option<Tag> {
        let name=self.unquoted()?.to_string_lossy();let mut args=None;
        if let Some(values)=self.attempt(|p| {p.ch('(')?;let mut values=Vec::new();if let Some(v)=p.attempt(Self::literal) {values.push(v);while p.optional_char(',') {if let Some(v)=p.attempt(Self::literal) {values.push(v)}else {break}}}p.ch(')')?;Some(values)}) {args=Some(values)}
        if name.starts_with(|c:char|c.is_ascii_digit()||matches!(c,'+'|'-'|'.')) {return self.fail("Invalid start of an unquoted string")}
        if let Some(values)=args {
            if values.len()==1&&name=="bool" {return if let Some(value)=values[0].as_double() {Some(Tag::Byte(i8::from(value!=0.0)))}else {self.fail("Expected a number or a boolean")}}
            if values.len()==1&&name=="uuid" {let value=values[0].as_string().and_then(|s|parse_uuid(&s.to_string_lossy()));return if let Some(v)=value {Some(Tag::IntArray(v))}else {self.fail("Expected a string representing a valid UUID")}}
            return self.fail(format!("No such operation: {name}/{}",values.len()))
        }
        Some(if name.eq_ignore_ascii_case("true") {Tag::Byte(1)}else if name.eq_ignore_ascii_case("false") {Tag::Byte(0)}else {Tag::String(name.into())})
    }
    fn literal(&mut self)->Option<Tag> {
        self.ws();if self.depth>=512 {return self.fail("NBT value is too deeply nested")}
        self.depth+=1;let result=self.literal_inner();self.depth-=1;result
    }
    fn literal_inner(&mut self)->Option<Tag> {
        if self.peek().is_some_and(|c|matches!(c,43|45|46|48..=57)) {
            if let Some(value)=self.attempt(Self::float) {return Some(value)}
            if let Some(value)=self.attempt(|p|{let n=p.integer()?;p.number(&n,'i')}) {return Some(value)}
        }
        match self.peek() {Some(34|39)=>self.quoted().map(Tag::String),Some(123)=>self.compound(),Some(91)=>self.list(),_=>self.builtin()}
    }
}
fn java_whitespace(c:u16)->bool {matches!(c,9..=13|28..=32|0x1680|0x2000..=0x2006|0x2008..=0x200a|0x2028|0x2029|0x205f|0x3000)}
fn parse_uuid(s:&str)->Option<Vec<i32>> {
    if s.len()>36 {return None}let parts:Vec<_>=s.split('-').collect();if parts.len()!=5 {return None}
    let mut numbers=Vec::new();for p in parts {let p=p.strip_prefix('+').unwrap_or(p);numbers.push(u64::from_str_radix(p,16).ok()?)}
    let high=((numbers[0]&0xffffffff)<<32)|((numbers[1]&0xffff)<<16)|(numbers[2]&0xffff);
    let low=((numbers[3]&0xffff)<<48)|(numbers[4]&0xffffffffffff);
    Some(vec![(high>>32) as i32,high as i32,(low>>32) as i32,low as i32])
}

/// Port of `StringTag#quoteAndEscape`.
pub fn quote(value:&NbtString)->String {
    let text=value.to_string_lossy();let quote=text.chars().find_map(|c|match c {'"'=>Some('\''),'\''=>Some('"'),_=>None}).unwrap_or('"');let mut out=String::new();out.push(quote);
    for c in text.chars() {match c {'\\'=>out.push_str("\\\\"),'\x08'=>out.push_str("\\b"),'\t'=>out.push_str("\\t"),'\n'=>out.push_str("\\n"),'\x0c'=>out.push_str("\\f"),'\r'=>out.push_str("\\r"),c if c<' '=>out.push_str(&format!("\\x{:02X}",c as u32)),c=>{if c==quote {out.push('\\')}out.push(c)}}}out.push(quote);out
}
fn key(value:&NbtString,strict:bool)->String {let text=value.to_string_lossy();let simple=!text.is_empty()&&text.chars().all(|c|c.is_ascii_alphanumeric()||matches!(c,'.'|'_'|'+'|'-'));
    let strict_ok=text.starts_with(|c:char|c.is_ascii_alphabetic()||c=='.'||c=='_')&&!text.eq_ignore_ascii_case("true")&&!text.eq_ignore_ascii_case("false");
    if simple&&(!strict||strict_ok) {text}else {quote(value)}
}
fn float_string(text:String)->String {
    if matches!(text.as_str(),"NaN"|"inf"|"-inf") {return text.replace("-inf","-Infinity").replace("inf","Infinity")}
    let negative=text.starts_with('-');let unsigned=text.trim_start_matches('-');let (whole,fraction)=unsigned.split_once('.').unwrap_or((unsigned,""));
    let digits=format!("{whole}{fraction}");let leading=digits.bytes().take_while(|c|*c==b'0').count();
    if leading==digits.len() {return "0.0".into()}
    let exponent=whole.len() as i32-leading as i32-1;
    if !(-3..7).contains(&exponent) {let digits=digits[leading..].trim_end_matches('0');let tail=if digits.len()==1 {"0"}else {&digits[1..]};format!("{}{}.{}E{exponent}",if negative {"-"}else {""},&digits[..1],tail)}
    else if fraction.is_empty() {format!("{text}.0")}else {text}
}
fn scalar(tag:&Tag)->Option<String> {Some(match tag {Tag::End=>"END".into(),Tag::Byte(n)=>format!("{n}b"),Tag::Short(n)=>format!("{n}s"),Tag::Int(n)=>n.to_string(),Tag::Long(n)=>format!("{n}L"),Tag::Float(n)=>format!("{}f",float_string(n.to_string())),Tag::Double(n)=>format!("{}d",float_string(n.to_string())),Tag::String(s)=>quote(s),_=>return None})}
impl fmt::Display for Tag {fn fmt(&self,f:&mut fmt::Formatter<'_>)->fmt::Result {f.write_str(&compact(self))}}
impl fmt::Display for CompoundTag {fn fmt(&self,f:&mut fmt::Formatter<'_>)->fmt::Result {Tag::Compound(self.clone()).fmt(f)}}
impl fmt::Display for ListTag {fn fmt(&self,f:&mut fmt::Formatter<'_>)->fmt::Result {Tag::List(self.clone()).fmt(f)}}
fn compact(tag:&Tag)->String {
    if let Some(s)=scalar(tag) {return s}
    match tag {Tag::Compound(c)=>format!("{{{}}}",c.iter().map(|(k,v)|format!("{}:{}",key(k,true),compact(v))).collect::<Vec<_>>().join(",")),Tag::List(v)=>format!("[{}]",v.iter().map(compact).collect::<Vec<_>>().join(",")),_=>array_string(tag,false,false)}
}
fn array_string(tag:&Tag,spaces:bool,truncate:bool)->String {
    let (prefix,values)=match tag {Tag::ByteArray(v)=>("B",v.iter().map(|v|format!("{v}{}",if truncate {"b"}else {"B"})).collect::<Vec<_>>()),Tag::IntArray(v)=>("I",v.iter().map(ToString::to_string).collect()),Tag::LongArray(v)=>("L",v.iter().map(|v|format!("{v}L")).collect()),_=>unreachable!()};
    let mut out=format!("[{prefix};");let count=if truncate {values.len().min(128)}else {values.len()};for (i,value) in values.iter().take(count).enumerate() {if spaces {out.push(' ')}out.push_str(value);if i+1<values.len() {out.push(',')}}if count<values.len() {out.push_str("<...>")}out.push(']');out
}
/// Port of `SnbtPrinterTagVisitor`, including structure-template key ordering and indentation.
pub fn pretty(tag:&Tag)->String {pretty_inner(tag,"    ",0,&mut Vec::new())}
fn pretty_inner(tag:&Tag,indent:&str,depth:usize,path:&mut Vec<String>)->String {
    if let Some(value)=scalar(tag) {return if matches!(tag,Tag::End) {String::new()}else {value}}
    let (open,close)=match tag {Tag::Compound(c) if c.is_empty()=>return "{}".into(),Tag::List(v) if v.is_empty()=>return "[]".into(),Tag::Compound(_)=>("{","}"),Tag::List(_)=>("[","]"),_=>return array_string(tag,true,false)};
    path.push(if open=="{" {"{}"}else {"[]"}.into());let path_string=path.join(".");
    let indent=if matches!(path_string.as_str(),"{}.size.[]"|"{}.data.[].{}"|"{}.palette.[].{}"|"{}.entities.[].{}") {""}else {indent};
    let values=match tag {Tag::List(v)=>v.iter().map(|v|pretty_inner(v,indent,depth+1,path)).collect::<Vec<_>>(),Tag::Compound(c)=>{
        let priority:&[&str]=match path_string.as_str() {"{}"=>&["DataVersion","author","size","data","entities","palette","palettes"],"{}.data.[].{}"=>&["pos","state","nbt"],"{}.entities.[].{}"=>&["blockPos","pos"],_=>&[]};
        let mut keys=c.iter().map(|(k,_)|k).collect::<Vec<_>>();keys.sort_by_key(|k|priority.iter().position(|p|**k==NbtString::from(*p)).unwrap_or(priority.len()));
        keys.into_iter().map(|k|{path.push(k.to_string_lossy());let result=format!("{}: {}",key(k,false),pretty_inner(c.get_key(k).unwrap(),indent,depth+1,path));path.pop();result}).collect()
    },_=>unreachable!()};path.pop();format_container(open,close,values,indent,depth)
}
fn format_container(open:&str,close:&str,values:Vec<String>,indent:&str,depth:usize)->String {if indent.is_empty() {format!("{open}{}{close}",values.join(", "))}else {format!("{open}\n{}\n{}{close}",values.into_iter().map(|v|format!("{}{v}",indent.repeat(depth+1))).collect::<Vec<_>>().join(",\n"),indent.repeat(depth))}}
/// Plain text from `TextComponentTagVisitor`, as used by `/data get`. `sort_keys` mirrors its debug option.
pub fn to_text(tag:&Tag,indent:&str,sort_keys:bool)->String {text_inner(tag,indent,sort_keys,0,0)}
fn text_inner(tag:&Tag,indent:&str,sort:bool,depth:usize,indent_depth:usize)->String {
    if let Some(s)=scalar(tag) {return if matches!(tag,Tag::End) {String::new()}else {s}}
    match tag {
        Tag::List(v)=>{if v.is_empty() {return "[]".into()}if depth>=64 {return "[<...>]".into()}
            let wrap=v.len()<8&&v.iter().any(|t|t.as_double().is_none());let values=v.iter().map(|t|text_inner(t,indent,sort,depth+1,indent_depth+usize::from(wrap))).collect::<Vec<_>>();format_container("[","]",values,if wrap {indent}else {""},indent_depth)
        },
        Tag::Compound(c)=>{if c.is_empty() {return "{}".into()}if depth>=64 {return "{<...>}".into()}
            let keys=if sort {c.iter().map(|(k,_)|k).collect()}else {c.java_keys()};let values=keys.into_iter().map(|k|format!("{}: {}",key(k,false),text_inner(c.get_key(k).unwrap(),indent,sort,depth+1,indent_depth+1))).collect();format_container("{","}",values,indent,indent_depth)
        },_=>array_string(tag,true,true),
    }
}
