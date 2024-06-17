// See README.md for simple usage and motivation
// below are gory implementation details
//
// First, we need the JSON schema. We get this via
// `cargo run --example generate-schema`
// from the `packages/msg` dir
// (or, if you already have the binary built, run `target/release/examples/generate-schema`)
//
// That's not so bad...
// the raw json entry files are, however, buggy and weird, and their format changed recently
//
// basically, the schema generator writes the schema data to a single file which has valid schema
// on the inside objects, but the file itself is *not* a valid schema, so it cannot be parsed directly
//
// also, up until a very recent update, the response files were not even in that top-level file
// rather, they need to be copied over from the raw folder
// and hope that unlike the raw entry files, these are not corrupted (i.e. they needed a different approach to parse)
// since this is a very recent update, we are currently maintaining a bit of backwards compatibility, just in case
//
// regardless, even if it's a separate entry in the object, there are all separate schemas, and need to be parsed as such
// so we end up with a bunch of separate files
//
// so ultimately, it's a bit of song and dance, but we end up writing out:
//
// 1. top-level file which should be loaded and then the inner contents contains schema data
// 2. additional response_to files which are valid schema
//
// and this is then converted to typescript, without regard for the noise of separate files
// consumers/bundlers should re-export or whatever
import { compile, compileFromFile } from 'json-schema-to-typescript';
import * as path from "path";
import * as fsImport from "fs-extra";

// quick hack...
const fsExtra = { ...fsImport, ...fsImport.default.promises, ...fsImport.default };


(async () => {
    const srcDir = path.resolve(`../schema/json`);
    const outputDir = path.resolve(`../schema/typescript`);

    if(!fsExtra.existsSync(srcDir)) {
        throw new Error("must generate the JSON schema first");
    }

    await generateTs(srcDir, outputDir);
})();

async function generateTs(srcFolder, targetFolder, combineFolder = false) {
    if (!fsExtra.existsSync(srcFolder)) {
        throw new Error(`schema folder does not exist!`)
    }

    const schemaOptions = {
        additionalProperties: false
    }

    const files = await fsExtra.readdir(srcFolder);

    for (const file of files) {
        const src = path.resolve(`${srcFolder}/${file}`);

        if (fsExtra.lstatSync(src).isDirectory()) {
            await generateTs(src, path.resolve(`${targetFolder}/${file}`))
        } else {
            fsExtra.mkdirpSync(path.resolve(targetFolder));
            const src = path.resolve(`${srcFolder}/${file}`);

            console.log("processing", src);
            const contents = await fsExtra.readFile(src, "utf8");
            const schemas = JSON.parse(contents);
            const { contract_name } = schemas;
            if (!contract_name || contract_name === "") {
                const ts = await compileFromFile(src, schemaOptions);
                const basename = path.basename(src);
                const dest = path.resolve(`${targetFolder}/${basename.replace(".json", ".ts")}`);
                fsExtra.writeFileSync(dest, ts);
            } else {
                // The top-level export is not a valid JSON schema
                // rather, gotta grab the object and parse individually
                for (const [key, data] of Object.entries(schemas)) {
                    if (data?.hasOwnProperty("$schema")) {
                        const name = `${contract_name}_${key}`;
                        const ts = await compile(data, name, schemaOptions);
                        const dest = path.resolve(`${targetFolder}/${name}.ts`);

                        fsExtra.writeFileSync(dest, ts);
                    }
                }
                // newest version adds the responses under responses...
                if (schemas.hasOwnProperty("responses")) {
                    for (const [key, data] of Object.entries(schemas.responses)) {
                        if (data?.hasOwnProperty("$schema")) {
                            const name = `${contract_name}_response_to_${key}`;
                            const ts = await compile(data, name, schemaOptions);
                            const dest = path.resolve(`${targetFolder}/${name}.ts`);

                            fsExtra.writeFileSync(dest, ts);
                        }
                    }
                }
            }
        }
    }
}

function formatData(data) {
    if (typeof data?.toString === "function") {
        return data.toString();
    } else {
        return data;
    }
}
