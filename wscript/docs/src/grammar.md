# Grammar Reference

This is an informal EBNF grammar for SpiteScript. The parser implementation is authoritative — this reference is for quick lookup.

## Program Structure

```ebnf
program         = item* EOF

item            = fn_decl
                | struct_decl
                | enum_decl
                | trait_decl
                | impl_block
                | const_decl
```

## Declarations

```ebnf
fn_decl         = attr* 'fn' IDENT generic_params? '(' params? ')' return_type? block

struct_decl     = attr* 'struct' IDENT generic_params? '{' struct_fields '}'
struct_fields   = (IDENT ':' type (',' IDENT ':' type)* ','?)?

enum_decl       = attr* 'enum' IDENT generic_params? '{' enum_variants '}'
enum_variants   = (enum_variant (',' enum_variant)* ','?)?
enum_variant    = attr* IDENT                              // unit
                | attr* IDENT '(' type (',' type)* ')'    // tuple
                | attr* IDENT '{' struct_fields '}'        // struct

trait_decl      = attr* 'trait' IDENT '{' trait_item* '}'
trait_item      = fn_decl | fn_sig ';'

impl_block      = 'impl' generic_params? type ('for' type)? '{' fn_decl* '}'

const_decl      = 'const' IDENT ':' type '=' expr ';'
```

## Parameters and Types

```ebnf
params          = param (',' param)*
param           = ('&' 'mut'? 'self') | (IDENT ':' type ('=' expr)?)
return_type     = '->' type

type            = 'i8' | 'i16' | 'i32' | 'i64' | 'i128'
                | 'u8' | 'u16' | 'u32' | 'u64' | 'u128'
                | 'f32' | 'f64' | 'bool' | 'char' | 'String'
                | type '[]'
                | 'Map' '<' type ',' type '>'
                | 'Option' '<' type '>'
                | 'Result' '<' type (',' type)? '>'
                | 'Fn' '(' type_list? ')' '->' type
                | 'Ref' '<' type '>'
                | '(' type_list? ')'                       // tuple or unit
                | IDENT generic_args?                      // named type

generic_params  = '<' IDENT (':' trait_bound ('+' trait_bound)*)? (',' ...)* '>'
generic_args    = '<' type (',' type)* '>'
trait_bound     = IDENT generic_args?
```

## Statements

```ebnf
block           = '{' stmt* '}'
stmt            = let_stmt | expr_stmt | return_stmt | for_stmt | while_stmt | loop_stmt
let_stmt        = 'let' 'mut'? (IDENT | tuple_pattern) (':' type)? '=' expr ';'
expr_stmt       = expr ';'
return_stmt     = 'return' expr? ';'
for_stmt        = 'for' (IDENT | tuple_pattern) 'in' expr block
while_stmt      = 'while' expr block
loop_stmt       = 'loop' block
```

## Expressions

Ordered by precedence (lowest to highest):

```ebnf
expr            = assignment_expr
assignment_expr = pipe_expr ('=' | '+=' | '-=' | '*=' | '/=' | '%=') assignment_expr
                | pipe_expr
pipe_expr       = or_expr ('|>' call_tail)*
or_expr         = and_expr ('||' and_expr)*
and_expr        = eq_expr ('&&' eq_expr)*
eq_expr         = cmp_expr (('==' | '!=') cmp_expr)*
cmp_expr        = bitor_expr (('<' | '>' | '<=' | '>=' | '<=>') bitor_expr)*
bitor_expr      = bitxor_expr ('|' bitxor_expr)*
bitxor_expr     = bitand_expr ('^' bitand_expr)*
bitand_expr     = shift_expr ('&' shift_expr)*
shift_expr      = add_expr (('<<' | '>>') add_expr)*
add_expr        = mul_expr (('+' | '-') mul_expr)*
mul_expr        = unary_expr (('*' | '/' | '%') unary_expr)*
unary_expr      = ('-' | '!' | 'not') unary_expr | postfix_expr
postfix_expr    = primary_expr (method_call | index | field | '?' | 'as' type)*
```

## Primary Expressions

```ebnf
primary_expr    = literal | ident_expr | call_expr | if_expr | match_expr
                | lambda | block | range | paren_expr | array_lit | map_lit
                | macro_call

ident_expr      = IDENT (('::' IDENT)* struct_init?)?
call_expr       = ident_expr '(' named_args? ')'
method_call     = '.' IDENT generic_args? '(' args? ')'
index           = '[' expr ']'
field           = '.' (IDENT | INT)

if_expr         = 'if' expr block ('else' 'if' expr block)* ('else' block)?
match_expr      = 'match' expr '{' match_arm (',' match_arm)* ','? '}'
match_arm       = pattern ('if' expr)? '=>' (expr | block)

lambda          = '|' lambda_params? '|' (expr | block)
range           = expr '..' expr | expr '..=' expr
```

## Patterns

```ebnf
pattern         = '_' | literal | IDENT | tuple_pattern
                | IDENT '::' IDENT pattern_payload?
                | IDENT '{' field_patterns '}'
                | pattern '@' IDENT

tuple_pattern   = '(' (IDENT | '_') (',' (IDENT | '_'))* ')'
pattern_payload = '(' pattern (',' pattern)* ')' | '{' field_patterns '}'
field_patterns  = (IDENT (':' pattern)?)* (',' '..')?
```

## Literals

```ebnf
literal         = INT_LIT | FLOAT_LIT | BOOL_LIT | CHAR_LIT | STR_LIT | TEMPLATE_LIT
array_lit       = '[' (expr (',' expr)* ','?)? ']'
map_lit         = '#' '{' (expr ':' expr (',' expr ':' expr)* ','?)? '}'
struct_init     = '{' (IDENT (':' expr)? (',' IDENT (':' expr)?)* ','? ('..' expr)?)? '}'
```

## Attributes and Macros

```ebnf
attr            = '@' IDENT ('(' attr_args ')')?
attr_args       = attr_arg (',' attr_arg)*
attr_arg        = IDENT (':' literal)? | literal

macro_call      = IDENT '!' '(' macro_args ')'
macro_args      = (expr (',' expr)* ','?)?
```

## Keywords

```
let  mut  const  fn  return  if  else  match  for  in  while  loop
break  continue  struct  impl  trait  enum  true  false  as
and  or  not  pub  self  Self  None  Some  Ok  Err
```

## Operators

```
+  -  *  /  %           arithmetic
==  !=  <  >  <=  >=    comparison
&&  ||  !               logical (also: and, or, not)
&   |   ^   ~   <<  >>  bitwise
=   +=  -=  *=  /=  %=  assignment
|>                      pipe
?                       error propagation
<=>                     spaceship (three-way comparison)
..   ..=                range (exclusive, inclusive)
.                       member access
::                      path separator
->                      return type annotation
=>                      match arm
@                       attribute prefix
```
