#!/bin/bash

#Really hacky way to build everything until I figure out how to get a better/
#dependency-respecting build system going

rm -r build/dist
wasm-pack build --target=no-modules --out-dir=extension/build/dist ..
npx webpack