## 課題2

課題1 のプログラム実行時に、入力値に不備がないか検証できるようにしてください。

入力値に不備がないかは、スキーマファイルによって検証することとします。

```
endpoint -> string
debug -> bool
log.file -> string
```

上述のようなスキーマファイルをユーザーが作成し、parseする対象のファイルとともにロードする事で、スキーマに従っていない場合にはエラーを出力するようにしたいです。

上記のスキーマは一例なので、自由に形式を考えてください。

### 仕様

各行は `key -> type` の形式で記述されています。
`type` は以下のいずれかです。

- string
- bool
- int
- float

sysctl.confとは異なり、コメントアウトや`-`によるエラーの無視等はありません。ただし、空行は無視します。
