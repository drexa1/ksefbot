// demo encryption/decryption script
// put public key in public_key.pem file in the same folder as this script
// put private key in private_key.pem file in the same folder as this script
import crypto from "crypto";
import fs from "fs";

const CLEAR_AES_KEY_STR = "<some-key-data>"
const PRIVATE_KEY_DATA = fs.readFileSync("private_key.pem", "utf8");
const PUBLIC_KEY_DATA = fs.readFileSync("public_key.pem", "utf8");

const encryptedAesKey = crypto.publicEncrypt({
    key: PUBLIC_KEY_DATA,
    padding: crypto.constants.RSA_PKCS1_OAEP_PADDING,
    oaepHash: "sha256"
}, Buffer.from(CLEAR_AES_KEY_STR));
const encryptedAesKeyBase64 = Buffer.from(encryptedAesKey).toString("base64");
const decryptedAesKey = crypto.privateDecrypt({
    key: crypto.createPrivateKey({
        key: PRIVATE_KEY_DATA,
        format: "pem",
        type: "pkcs1",
        passphrase: "<passphrase>"
    }),
    padding: crypto.constants.RSA_PKCS1_OAEP_PADDING,
    oaepHash: "sha256"
}, Buffer.from(encryptedAesKeyBase64, "base64"));

if (decryptedAesKey.toString() === CLEAR_AES_KEY_STR) {
    console.log("Success, keys match!")
} else {
    console.log("Failed, keys do not match!")
}