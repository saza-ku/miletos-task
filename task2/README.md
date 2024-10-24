課題1 のプログラム実行時に、入力値に不備がないか検証できるようにしてください。

入力値に不備がないかは、スキーマファイルによって検証することとします。

```
endpoint -> string
debug -> bool
log.file -> string
```

上述のようなスキーマファイルをユーザーが作成し、parseする対象のファイルとともにロードする事で、スキーマに従っていない場合にはエラーを出力するようにしたいです。

上記のスキーマは一例なので、自由に形式を考えてください。