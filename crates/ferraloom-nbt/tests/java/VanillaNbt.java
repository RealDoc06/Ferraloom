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
