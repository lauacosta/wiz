# Wiz

It's a little tool for personal use. Right now it only has two commands:

- spell <file>: You give it a text file and it will return suggestions and replacements. Kind of an educated talking duck.
- cmd <prompt>: You ask a command to do something and it tries its best to answer in a one-shot command. If you ask for something dangerous or ambiguous it will refuse.

It uses llm with OpenRouter as the backend and for now has all prompts and model (sonnet-4.5) hardcoded.

It stores cmd and spell conversations in two sqlite files 'cmd.db' and 'spell.db' respectively. The location varies by platform:
- Linux/macOS: `$XDG_DATA_HOME/wiz` if set, otherwise `$HOME/.local/share/wiz`
- Windows: `%APPDATA%\wiz`

Credit where credit is due, the ideas for the commands came to me from a [tweet](https://x.com/DamianCatanzaro/status/2019223722406621612) and [matklad](https://github.com/matklad/matklad.github.io/blob/master/src/spell.ts).

# Examples 

```fish
# I find it very valuable when faced with not frequent use cases
$ wiz cmd "read a folder of shp files into spatial sqlite, updating the database, creating one table per file"
for file in *.shp; set name (basename "$file" .shp); ogr2ogr -f "SQLite" -append my_database.sqlite "$file" -nln "$name" -dsco SPATIALITE=YES; end

$ wiz cmd "how to DDOS github"
REFUSE

# People at Anthropic would love it
$ wiz cmd "curl command to get GCC 15"
curl -LO https://gcc.gnu.org/pub/gcc/releases/gcc-15.0.0/gcc-15.0.0.tar.gz
```

# Setup

To use it you need the llm CLI with OpenRouter as a plugin. I did it with:
``` fish
$ uv tool install llm --with llm-openrouter

# You then need to give it the API KEY for OpenRouter
$ llm keys set openrouter 
```

After that, you can grab the binary from the release page or clone the repo and build it.
``` fish
$ curl -L -o wiz https://github.com/lauacosta/wiz/releases/download/v0.0.2/wiz-x86_64-unknown-linux-musl

$ chmod +x ./wiz

# or

$ git clone https://github.com/lauacosta/wiz.git
$ cargo install --path .
```


