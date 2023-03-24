
# luac-parser (中文)

lua字节码解析器, 目前支持 lua51, lua53, lua54, luajit

这是目前效果最好的lua反编译器 [metaworm's luadec](http://luadec.metaworm.site) 的一部分

可以基于此代码定制你所需的lua字节码解析器，编译成WASM，让[metaworm's luadec][luadec]加载使用，来反编译非官方的lua字节码

得益于[nom][nom]库的灵活性，编写定制的解析器是很简单的一件事情，可以看[这篇文章][write-parser]了解如何编写

# luac-parser (in English)

lua bytecode parser, currently support lua51, lua53, lua54, luajit

This is part of [metaworm's luadec][luadec], which is the best lua decompiler at present

You can write your custom luac parser based on this code, which can be able to compiling to WASM and loaded by [metaworm's luadec][luadec], to decompile the unofficial lua bytecode

[luadec]: http://luadec.metaworm.site
[nom]: https://github.com/rust-bakery/nom

Thanks to the flexibility of [nom][nom], it is very easy to write your own parser in rust, read [this article][write-parser] to learn how to write a luac parser

[luadec]: http://luadec.metaworm.site
[nom]: https://github.com/rust-bakery/nom
[write-parser]: https://github.com/metaworm/luac-parser-rs/wiki/Write-custom-luac-parser