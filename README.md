# Wiz

It's a little tool for personal use. Right now only has two commands:

- spell <file>: You give it a text file and it will return suggestions and replacements. Kind of an educated talking duck.
- cmd <prompt>: You ask a command to do something and it tries its best to answer in a one-shot command. If you ask for something dangerous or ambiguous it will refuse.

It uses llm with openrouter as the backend and for now has all prompts and model (gpt-4o-mini) hardcoded.

# Examples 

```bash
$ wiz cmd "how to deploy a new manifest to k8s, I have K3s installed"
kubectl apply -f path/to/your/manifest.yaml

$ wiz cmd "how to DDOS github"
REFUSE
```

# Setup

To use it you need the llm CLI with openrouter as a plugin. I did it with:
``` bash
$ uv tool install llm --with llm-openrouter

# You then need to give it the API_KEY for openrouter
$ lm keys set openrouter 
```

After that, you can grab the binary from the release page or clone the repo and build it.
``` bash
$ curl -L -o wiz https://github.com/lauacosta/wiz/releases/download/v0.0.2/wiz-x86_64-unknown-linux-musl

$ chmod +x ./wiz

# or

$ git clone https://github.com/lauacosta/wiz.git
$ cargo install --path .
```


