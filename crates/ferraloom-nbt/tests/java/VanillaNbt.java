import java.io.*;
import java.nio.charset.StandardCharsets;
import java.util.*;
import java.util.zip.*;
import net.minecraft.nbt.*;
import net.minecraft.commands.arguments.NbtPathArgument.NbtPath;
import com.mojang.brigadier.exceptions.CommandSyntaxException;

/** Generates test vectors using the real 26.3 classes. Output is TSV with UTF-8 hex strings. */
public class VanillaNbt {
    static final TagParser<Tag> PARSER = TagParser.create(NbtOps.INSTANCE);
    static String hex(String s) {return HexFormat.of().formatHex(s.getBytes(StandardCharsets.UTF_8));}
    static void line(String... fields) {System.out.println(String.join("\t", fields));}
    static void parse(String input) {
        try {
            Tag tag=PARSER.parseFully(input);
            var bytes=new ByteArrayOutputStream();
            NbtIo.writeAnyTag(tag,new DataOutputStream(bytes));
            line("parse",hex(input),hex(tag.toString()),HexFormat.of().formatHex(bytes.toByteArray()),
                hex(new SnbtPrinterTagVisitor().visit(tag)),hex(new TextComponentTagVisitor("",TextComponentTagVisitor.PlainStyling.INSTANCE,true).visit(tag).getString()));
        }catch(CommandSyntaxException e) {line("error",hex(input),Integer.toString(e.getCursor()),hex(e.getRawMessage().getString()));}
        catch(Exception e) {throw new RuntimeException(e);}
    }
    static void path(String input,String operation,String value) throws Exception {
        CompoundTag root=TagParser.parseCompoundFully("{a:[{id:1,x:2},{id:2,x:3}],n:[I;1,2],v:4}");
        try {
            NbtPath p=NbtPath.of(input);
            String result=switch(operation) {
                case "get" -> p.get(root).toString();
                case "set" -> Integer.toString(p.set(root,PARSER.parseFully(value)));
                case "insert" -> Integer.toString(p.insert(-1,root,List.of(PARSER.parseFully(value))));
                default -> Integer.toString(p.remove(root));
            };
            line("path",hex(input),operation,hex(value),hex(result),hex(root.toString()));
        }catch(CommandSyntaxException e) {line("patherror",hex(input),operation,hex(value),Integer.toString(e.getCursor()),hex(e.getRawMessage().getString()));}
    }
    static void account(String hex,long quota,int depth) throws Exception {
        NbtAccounter a=new NbtAccounter(quota,depth);
        try {
            Tag tag=NbtIo.readAnyTag(new DataInputStream(new ByteArrayInputStream(HexFormat.of().parseHex(hex))),a);
            line("account",hex,Long.toString(quota),Integer.toString(depth),Long.toString(a.getUsage()),Integer.toString(a.getDepth()),"ok",hex(tag.toString()));
        }catch(Exception e) {
            Throwable cause=e;
            if(e instanceof ReportedNbtException) while(cause.getCause()!=null) cause=cause.getCause();
            line("account",hex,Long.toString(quota),Integer.toString(depth),Long.toString(a.getUsage()),Integer.toString(a.getDepth()),"error",hex(cause.getMessage()));
        }
    }
    public static void main(String[] args) throws Exception {
        if(args.length>0) {for(String s:args)parse(s);return;}
        String[] cases={"{}","[]","[1,2b,foo,{a:2}]","{z:1,a:2,True:3,1:4,\"x y\":5}",
            "[B;0,127,-128,255ub]","[I;1b,2s,0xffffffff]","[L;1,2b,3s,4i,0xffffffffffffffff]",
            "[B;1s]","[I;1L]","[L;1.0]","[true,false,TRUE,False]","{a:1,a:2,}","[1,2,]",
            "0xff","0xffsb","0b11111111b","255ub","65535us","4294967295ui","18446744073709551615ul",
            "-0xff","-0xffsi","-1ub","+2s","1__000","01","00.5","1_","_1","0x_1","0b2","1e3","1.e2",".5f","1.","-0.0f","1e999","NaN","Infinity","1foo","123abc","+foo",".foo",
            "bool(256)","bool(1.5)","bool(-0.1)","bool(\"true\")","uuid(\"123e4567-e89b-12d3-a456-426614174000\")","uuid(\"1-1-1-1-1\")","uuid(1)","bool()","unknown(1)",
            "\"\\b\\s\\t\\n\\f\\r\\\\\\'\\\"\\x41\\u0042\\U0001F600\\N{LATIN CAPITAL LETTER C}\"",
            "\"\\q\"","\"\\x0\"","\"\\U00110000\"","\"\\N{NO SUCH NAME}\"","\"unterminated","{\"\":1}","{a}","[1 2]","","{a:}","{a:1} x",
            "{DataVersion:1,size:[1,2,3],data:[{nbt:{z:1},state:0,pos:[1,2,3]}],entities:[{pos:[1d,2d,3d],blockPos:[1,2,3]}],palette:[{Name:stone}],z:4}",
            "{s:\"nul\\x00é😀\",b:1b,sht:2s,i:3,l:4L,f:1.25f,d:2.5d,ba:[B;1,-2],ia:[I;3,-4],la:[L;5,-6],list:[1,foo]}"};
        for(String s:cases)parse(s);
        for(String s:List.of("1__2.3__4e+0_2f","1 f","- 1","0 x FF","[B;1,2b,]","{a:1, b:2}","\"a \\n b\"","128b","256ub","32768s","2147483648","9223372036854775808L","4294967296ui","18446744073709551616ul","-1ui","1e999f"))parse(s);
        for(String bytes:List.of("00","0101","020002","0300000003","040000000000000004","053f800000","063ff0000000000000","08000161","0903000000020000000100000002","0a030001610000000100","090000000001","0901ffffffff","07ffffffff","0aff00016100","0a0a0001780000")) {
            for(long quota:List.of(0L,8L,36L,128L,1024L)) account(bytes,quota,512);
        }
        account("0a0a0001780000",1024,1);
        account("0a0a0001780000",1024,0);
        for(String bytes:List.of("0000","0001ff","0001c0","0002c080","00020000","0003eda080","0004f09f9880","0003e18000","0002c000")) {
            try {String value=new DataInputStream(new ByteArrayInputStream(HexFormat.of().parseHex(bytes))).readUTF();StringBuilder units=new StringBuilder();for(char c:value.toCharArray())units.append(String.format("%04x",(int)c));line("utf",bytes,"ok",units.toString());}
            catch(UTFDataFormatException e) {line("utf",bytes,"error",hex(e.getMessage()));}
        }
        for(Tag tag:List.of(IntTag.valueOf(256),LongTag.valueOf(Long.MAX_VALUE),FloatTag.valueOf(-1.9f),DoubleTag.valueOf(-1.9),DoubleTag.valueOf(Double.NEGATIVE_INFINITY),FloatTag.valueOf(Float.NaN))) {
            NumericTag n=(NumericTag)tag;
            line("numeric",hex(tag.toString()),Integer.toString(n.byteValue()),Integer.toString(n.shortValue()),Integer.toString(n.intValue()),Long.toString(n.longValue()),Boolean.toString(n.asBoolean().get()));
        }
        for(int length:List.of(65535,65536)) {
            String value="a".repeat(length);var out=new ByteArrayOutputStream();String error="";
            try {NbtIo.writeAnyTag(StringTag.valueOf(value),new DataOutputStream(out));}catch(UTFDataFormatException e) {error=e.getMessage();}
            var fallback=new ByteArrayOutputStream();NbtIo.writeUnnamedTagWithFallback(StringTag.valueOf(value),new DataOutputStream(fallback));
            line("fallback",Integer.toString(length),Integer.toString(out.size()),hex(error),Integer.toString(fallback.size()),HexFormat.of().formatHex(Arrays.copyOfRange(fallback.toByteArray(),0,Math.min(5,fallback.size()))));
        }
        Random random=new Random(263);
        for(int i=0;i<128;i++) {
            int bits=random.nextInt();float f=Float.intBitsToFloat(bits);
            if(Float.isFinite(f))line("float",Integer.toUnsignedString(bits,16),hex(FloatTag.valueOf(f).toString()));
            long dbits=random.nextLong();double d=Double.longBitsToDouble(dbits);
            if(Double.isFinite(d))line("double",Long.toUnsignedString(dbits,16),hex(DoubleTag.valueOf(d).toString()));
        }
        for(float f:new float[]{Float.MIN_VALUE,Float.MIN_NORMAL,Float.MAX_VALUE,1e-4f,1e7f,1e23f})line("float",Integer.toUnsignedString(Float.floatToRawIntBits(f),16),hex(FloatTag.valueOf(f).toString()));
        for(double d:new double[]{Double.MIN_VALUE,Double.MIN_NORMAL,Double.MAX_VALUE,1e-4,1e7,1e23})line("double",Long.toUnsignedString(Double.doubleToRawLongBits(d),16),hex(DoubleTag.valueOf(d).toString()));
        ListTag big=new ListTag();for(int i=0;i<130;i++)big.add(IntTag.valueOf(i));
        ByteArrayTag array=new ByteArrayTag(new byte[130]);
        for(Tag tag:List.of(big,array))line("text",hex(tag.toString()),hex(new TextComponentTagVisitor("",TextComponentTagVisitor.PlainStyling.INSTANCE,true).visit(tag).getString()));
        for(String p:List.of("a[]","a[-1].x","a[{id:2}].x","{v:4}.a[0]","a[{id:9}]","missing.x","n[]","a[99]"))path(p,"get","");
        for(String p:List.of("new[].x","a[{id:2}].x","a[{id:9}].x","a[].x","n[]","a[-1]","v.x","obj{id:7}.x"))path(p,"set","8");
        for(String p:List.of("a","n","missing","v"))path(p,"insert","7b");
        for(String p:List.of("a[{id:1}]","a[].x","n[-1]","missing.x","{v:4}"))path(p,"remove","");
        for(String[] pair:new String[][]{{"{a:1}","{a:1,b:2}"},{"[1,1]","[1]"},{"[1,1]","[1,2]"},{"[]","[1]"},{"[2,1]","[1,2]"},{"1b","1"}}) {
            for(boolean partial:List.of(false,true))line("compare",hex(pair[0]),hex(pair[1]),Boolean.toString(partial),Boolean.toString(NbtUtils.compareNbt(PARSER.parseFully(pair[0]),PARSER.parseFully(pair[1]),partial)));
        }
        var compound=TagParser.parseCompoundFully("{s:\"a\\x00é😀\",l:[1,foo]}");
        for(String compression:List.of("none","gzip","zlib")) {
            var out=new ByteArrayOutputStream();
            OutputStream stream=switch(compression) {case "gzip"->new GZIPOutputStream(out);case "zlib"->new DeflaterOutputStream(out);default->out;};
            var data=new DataOutputStream(stream);NbtIo.write(compound,data);data.close();
            line("compressed",compression,HexFormat.of().formatHex(out.toByteArray()),hex(compound.toString()));
        }
    }
}
