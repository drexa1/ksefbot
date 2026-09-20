import {Env} from "../../worker";
import pRetry, {AbortError} from "p-retry";
import {AppUser} from "../../types/users";
import {AppContractor, KsefIdentifiable} from "../../types/contractors";
import {
    InvoiceEncryptionData,
    KsefAuthenticationStatus,
    KsefContextIdentifier,
    KsefInvoiceQueryResult
} from "../../types/ksef";
import * as asn1js from "asn1js";
import * as pkijs from "pkijs";
import {invoiceFromXml} from "../app/invoices";

class KsefClientBase {
    token?: string;

    constructor(protected env: Env) {}

    async authenticate(appUser: AppUser): Promise<void> {
        this.token = await this.getKsefToken(this.env, appUser);
        console.info("🪪 KSeF token acquired")
    }

    protected async getKsefEncryptionCertificate(env: Env, usage: "KsefTokenEncryption" | "SymmetricKeyEncryption"): Promise<{ certificate: string; publicKeyId: string }> {
        const response = await fetch(`${env.KSEF_URL}/security/public-key-certificates`);
        if (!response.ok)
            throw new Error(`KSeF certificates failed ${response.status}: ${await response.text()}`);
        const data = await response.json() as {
            certificate: string;
            usage: string[];
            publicKeyId: string;
            validFrom: string;
            validTo: string;
        }[];
        const now = Date.now();
        const cert = data.filter(c =>
            c.usage.includes(usage) &&
            new Date(c.validFrom).getTime() <= now &&
            new Date(c.validTo).getTime() > now
        ).sort((a, b) => new Date(b.validFrom).getTime() - new Date(a.validFrom).getTime())[0];
        if (!cert)
            throw new Error(`No valid KSeF certificate for ${usage}`);
        return { certificate: cert.certificate, publicKeyId: cert.publicKeyId };
    }

    private async getKsefToken(env: Env, user: AppUser): Promise<string> {
        console.info("1️⃣️ Requesting KSeF auth challenge...");
        const ksefChallenge = await this.getKsefChallenge(env);
        console.info("2️⃣️ Requesting KSeF public key certificates...");
        const ksefCertificate = await this.getKsefEncryptionCertificate(env, "KsefTokenEncryption");
        console.info("3️⃣️ Encrypting token...");
        const encryptedToken = await this.encryptKsefToken(user.ksefApiToken!, ksefChallenge.timestamp, ksefCertificate.certificate);
        console.info("4️⃣️ Requesting KSeF authentication...");
        const auth = await this.startKsefAuthentication(env, user, ksefChallenge.challenge, encryptedToken, ksefCertificate.publicKeyId);
        await this.waitForKsefAuthentication(env, auth.referenceNumber, auth.authenticationToken);
        console.info("5️⃣️ Redeeming KSeF token...");
        return await this.redeemKsefToken(env, auth.authenticationToken);
    }

    private async getKsefChallenge(env: Env): Promise<{ challenge: string, timestamp: number }> {
        const response = await fetch(`${env.KSEF_URL}/auth/challenge`, { method: "POST" });
        return await response.json() as { challenge: string, timestamp: number };
    }

    private async encryptKsefToken(token: string, timestamp: number, certificate: string): Promise<string> {
        const timestampMs = new Date(timestamp).getTime();
        const tokenPayload = new TextEncoder().encode(`${token}|${timestampMs}`);
        // PEM certificate -> DER bytes
        const certificateDer = Uint8Array.from(atob(certificate
            .replace(/-----BEGIN CERTIFICATE-----/, "")
            .replace(/-----END CERTIFICATE-----/, "")
            .replace(/\s/g, "")), c => c.charCodeAt(0));
        // Parse X.509 certificate
        const asn1 = asn1js.fromBER(certificateDer.buffer);
        if (asn1.offset === -1) throw new Error("Invalid certificate DER");
        const cert = new pkijs.Certificate({ schema: asn1.result });
        // Extract SubjectPublicKeyInfo
        const spki = cert.subjectPublicKeyInfo.toSchema().toBER();
        // Import RSA public key
        const publicKey = await crypto.subtle.importKey("spki", spki, { name: "RSA-OAEP", hash: "SHA-256"}, false, ["encrypt"]);
        // Encrypt token payload
        const encrypted = await crypto.subtle.encrypt({ name: "RSA-OAEP" }, publicKey, tokenPayload);
        return btoa(String.fromCharCode(...new Uint8Array(encrypted)));
    }

    private async startKsefAuthentication(env: Env, user: AppUser, challenge: string, encryptedToken: string, publicKeyId: string): Promise<{ referenceNumber: string, authenticationToken: string }> {
        const response = await fetch(`${env.KSEF_URL}/auth/ksef-token`, {
            method: "POST",
            headers: { "Content-Type": "application/json" },
            body: JSON.stringify({
                challenge,
                contextIdentifier: this.contextIdentifier(user),
                encryptedToken,
                publicKeyId
            })
        });
        if (!response.ok) {
            const body = await response.text();
            throw new Error(`KSeF authentication failed ${response.status}: ${body}`);
        }
        const data = await response.json() as { referenceNumber: string; authenticationToken: { token: string, validUntil: string }};
        return { referenceNumber: data.referenceNumber, authenticationToken: data.authenticationToken.token};
    }

    private contextIdentifier(user: AppUser): KsefContextIdentifier {
        if (user.id) return { type: "Nip", value: user.id };
        // REVIEW: NIP/Regon/Pesel are contractor data, no longer application user fields
        // if (user.pesel) return { type: "InternalId", value: user.pesel };
        throw new Error("Unsupported tax identifier");
    }

    private async waitForKsefAuthentication(env: Env, referenceNumber: string, authenticationToken: string): Promise<void> {
        await pRetry(
            async () => {
                const response = await fetch(`${env.KSEF_URL}/auth/${referenceNumber}`, {
                    headers: { Authorization: `Bearer ${authenticationToken}` }
                });
                if (!response.ok) throw new Error(`KSeF authentication status failed: ${response.status}`);
                const ksefAuthenticationStatus = await response.json() as KsefAuthenticationStatus;
                switch (ksefAuthenticationStatus.status.code) {
                    // Authentication in progress
                    case 100: throw new Error(`KSeF authentication pending: ${ksefAuthenticationStatus.status}`);
                    // Success
                    case 200: return;
                    default:  throw new AbortError(`${ksefAuthenticationStatus.status}}`);
                }
            }, { retries: 10, minTimeout: env.KSEF_MIN_TIMEOUT, maxTimeout: env.KSEF_MAX_TIMEOUT, factor: 1 }
        );
    }

    private async redeemKsefToken(env: Env, authenticationToken: string): Promise<string> {
        const response = await fetch(`${env.KSEF_URL}/auth/token/redeem`, { method: "POST", headers: {
            Authorization: `Bearer ${authenticationToken}`
        }});
        const tokens = await response.json() as { accessToken: { token: string }};
        return tokens.accessToken.token;
    }

    protected async importKsefPublicKey(certificate: string): Promise<CryptoKey> {
        const certificateDer = Uint8Array.from(atob(certificate
            .replace(/-----BEGIN CERTIFICATE-----/g, "")
            .replace(/-----END CERTIFICATE-----/g, "")
            .replace(/\s/g, "")), c => c.charCodeAt(0));
        const asn1 = asn1js.fromBER(certificateDer.buffer);
        if (asn1.offset === -1)
            throw new Error("Invalid KSeF certificate DER");
        const cert = new pkijs.Certificate({ schema: asn1.result });
        const spki = cert.subjectPublicKeyInfo.toSchema().toBER();
        return crypto.subtle.importKey("spki", spki, { name: "RSA-OAEP", hash: "SHA-256" }, false, ["encrypt"]);
    }

    protected async createInvoiceEncryptionData(publicKey: CryptoKey): Promise<InvoiceEncryptionData> {
        const cipherKey = crypto.getRandomValues(new Uint8Array(32));
        const cipherIv = crypto.getRandomValues(new Uint8Array(16));
        const encryptedSymmetricKey = await crypto.subtle.encrypt({ name: "RSA-OAEP" }, publicKey, cipherKey);
        return {
            cipherKey,
            cipherIv,
            encryptedSymmetricKey: this.arrayBufferToBase64(encryptedSymmetricKey),
            initializationVector: this.arrayBufferToBase64(cipherIv)
        };
    }

    protected async openOnlineSession(
        encryption: InvoiceEncryptionData,
        publicKeyId: string
    ): Promise<{ referenceNumber: string, validUntil: string }> {
        const response = await fetch(`${this.env.KSEF_URL}/sessions/online`, {
            method: "POST",
            headers: { Authorization: `Bearer ${this.token}`, "Content-Type": "application/json" },
            body: JSON.stringify({
                formCode: {
                    systemCode: "FA (3)",
                    schemaVersion: "1-0E",
                    value: "FA"
                },
                encryption: { encryptedSymmetricKey: encryption.encryptedSymmetricKey, initializationVector: encryption.initializationVector },
                publicKeyId
            })
        });
        if (!response.ok) {
            const body = await response.text();
            throw new Error(`KSeF online session failed ${response.status}: ${body}`);
        }
        return await response.json() as { referenceNumber: string, validUntil: string };
    }

    protected async sendOnlineInvoice(
        sessionReferenceNumber: string,
        invoiceBytes: Uint8Array,
        encryption: InvoiceEncryptionData
    ): Promise<{ referenceNumber: string }> {
        const encryptedInvoice = await this.encryptInvoiceXml(invoiceBytes, encryption.cipherKey, encryption.cipherIv);
        console.info("⚖️ Raw invoice bytes:", {
            length: invoiceBytes.length,
            firstBytes: Array.from(invoiceBytes.slice(0, 10)),
            lastBytes: Array.from(invoiceBytes.slice(-10)),
        });
        const invoiceHash = await this.sha256Base64(invoiceBytes);
        const encryptedInvoiceHash = await this.sha256Base64(encryptedInvoice);
        console.info("⚖️ KSeF invoice payload:", {
            sessionReferenceNumber,
            invoiceSize: invoiceBytes.length,
            invoiceHash,
            encryptedInvoiceSize: encryptedInvoice.length,
            encryptedInvoiceHash,
        });
        const response = await fetch(`${this.env.KSEF_URL}/sessions/online/${sessionReferenceNumber}/invoices`, {
            method: "POST",
            headers: { Authorization: `Bearer ${this.token}`, "Content-Type": "application/json"},
            body: JSON.stringify({
                invoiceHash,
                invoiceSize: invoiceBytes.length,
                encryptedInvoiceHash,
                encryptedInvoiceSize: encryptedInvoice.length,
                encryptedInvoiceContent: this.arrayBufferToBase64(encryptedInvoice)
            })
        });
        if (!response.ok) {
            const body = await response.text();
            throw new Error(`KSeF invoice submission failed ${response.status}: ${body}`);
        }
        return await response.json() as { referenceNumber: string };
    }

    private async encryptInvoiceXml(invoiceBytes: Uint8Array, cipherKey: Uint8Array, cipherIv: Uint8Array): Promise<Uint8Array> {
        const key = await crypto.subtle.importKey(
            "raw",
            this.toArrayBuffer(cipherKey),
            { name: "AES-CBC" },
            false,
            ["encrypt"]
        );
        // Web Crypto applies PKCS#7 padding automatically.
        const encrypted = new Uint8Array( await crypto.subtle.encrypt(
            { name: "AES-CBC", iv: this.toArrayBuffer(cipherIv) },
            key,
            this.toArrayBuffer(invoiceBytes)
        ));
        console.info("AFTER AES:", {
            plaintextLength: invoiceBytes.byteLength,
            ciphertextLength: encrypted.byteLength,
            expectedCiphertextLength: Math.ceil((invoiceBytes.byteLength + 1) / 16) * 16
        });
        return encrypted;
    }

    private async sha256Base64(data: Uint8Array): Promise<string> {
        const hash = await crypto.subtle.digest("SHA-256", this.toArrayBuffer(data));
        return this.base64FromBytes(new Uint8Array(hash));
    }

    private base64FromBytes(bytes: Uint8Array): string {
        let binary = "";
        const chunkSize = 0x8000;
        for (let i = 0; i < bytes.length; i += chunkSize) {
            binary += String.fromCharCode(...bytes.subarray(i, i + chunkSize));
        }
        return btoa(binary);
    }

    private toArrayBuffer(bytes: Uint8Array): ArrayBuffer {
        const buffer = new ArrayBuffer(bytes.byteLength);
        new Uint8Array(buffer).set(bytes);
        return buffer;
    }

    private arrayBufferToBase64(data: ArrayBuffer | Uint8Array): string {
        const bytes = data instanceof Uint8Array ? data : new Uint8Array(data);
        let binary = "";
        const chunkSize = 0x8000;
        for (let i = 0; i < bytes.length; i += chunkSize) {
            binary += String.fromCharCode(...bytes.subarray(i, i + chunkSize));
        }
        return btoa(binary);
    }

    protected async closeOnlineSession(sessionReferenceNumber: string): Promise<void> {
        const response = await fetch(`${this.env.KSEF_URL}/sessions/online/${sessionReferenceNumber}/close`, {
            method: "POST",
            headers: { Authorization: `Bearer ${this.token}` }
        });
        if (!response.ok) {
            const body = await response.text();
            throw new Error(`KSeF session close failed ${response.status}: ${body}`);
        }
    }
}

export class Client extends KsefClientBase {

    async queryPurchaseInvoices(env: Env, appUser: AppUser, subjectType: "Subject1" | "Subject2", from?: Date, to?: Date) {
        // Authentication
        await super.authenticate(appUser);
        // Query invoice metadata
        const metadataResult = await this.queryInvoiceMetadata(subjectType, from, to);
        console.info("📋️️ Downloaded invoices metadata...");
        // Download invoice XML files
        return await Promise.all(metadataResult.invoices.map(async invoiceMetadata => {
            const xmlContent = await this.downloadInvoice(invoiceMetadata.ksefNumber);
            console.info(`${subjectType === "Subject1" ? "💵" : "💳" }`+ "️ Downloaded invoice:", invoiceMetadata.invoiceNumber);
            return await invoiceFromXml(env, xmlContent, appUser, "sales", "Downloaded from KSeF");
        }));
    }

    private async queryInvoiceMetadata(subjectType: "Subject1" | "Subject2", from?: Date, to?: Date): Promise<KsefInvoiceQueryResult> {
        const dateRange = from || to ? {
            dateType: "Issue" as const,
            ...(from && { from: from.toISOString() }),
            ...(to && { to: to.toISOString() })
        } : undefined;
        const response = await fetch(`${this.env.KSEF_URL}/invoices/query/metadata`, {
            method: "POST",
            headers: {
                "Authorization": `Bearer ${this.token}`,
                "Content-Type": "application/json"
            },
            body: JSON.stringify({ subjectType, ...(dateRange && { dateRange })})
        });
        if (!response.ok) {
            const errorBody = await response.text();
            throw new Error(`KSeF query failed ${response.status}: ${errorBody}`);
        }
        return await response.json() as KsefInvoiceQueryResult;
    }

    private async downloadInvoice(ksefNumber: string): Promise<string> {
        const response = await fetch(`${this.env.KSEF_URL}/invoices/ksef/${ksefNumber}`, { headers: {
            Authorization: `Bearer ${this.token}`,
            Accept: "application/xml"
        }});
        if (!response.ok) {
            const errorBody = await response.text();
            throw new Error(`KSeF invoice download failed ${response.status}: ${errorBody}`);
        }
        return await response.text();
    }

    async postInvoice(appUser: AppUser, invoiceBytes: Uint8Array): Promise<{ sessionReferenceNumber: string, invoiceReferenceNumber: string }> {
        await this.authenticate(appUser);
        const { certificate, publicKeyId } = await this.getKsefEncryptionCertificate(this.env, "SymmetricKeyEncryption");
        const publicKey = await this.importKsefPublicKey(certificate);
        const encryption = await this.createInvoiceEncryptionData(publicKey);
        const session = await this.openOnlineSession(encryption, publicKeyId);
        console.info("KSeF online session opened:", session.referenceNumber);
        try {
            const status = await this.getSessionStatus(session.referenceNumber);
            console.info("KSeF initial session status:", JSON.stringify(status));
            if (status.status?.code !== 100)
                throw new Error(`KSeF session entered unexpected status: ${ status.status?.code } ${status.status?.description ?? ""}`);
            console.info("🚀 Sending invoice...");
            const invoice = await this.sendOnlineInvoice(session.referenceNumber, invoiceBytes, encryption);
            console.info("📥 Invoice accepted for processing:", invoice.referenceNumber);
            return { sessionReferenceNumber: session.referenceNumber, invoiceReferenceNumber: invoice.referenceNumber };
        } finally {
            try {
                await this.closeOnlineSession(session.referenceNumber);
                console.info("KSeF online session closed:", session.referenceNumber);
            } catch (error) {
                console.error("KSeF online session close failed:", error);
            }
        }
    }

    async getSessionStatus(sessionReferenceNumber?: string) {
        const url = sessionReferenceNumber
            ? `${this.env.KSEF_URL}/sessions/${sessionReferenceNumber}`
            : `${this.env.KSEF_URL}/sessions?pageSize=100&sessionType=Online`;
        const response = await fetch(url, { headers: { Authorization: `Bearer ${this.token}` }});
        const body = await response.text();
        if (!response.ok)
            throw new Error(`KSeF session status failed ${response.status}: ${body}`);
        return JSON.parse(body);
    }

    async getSessionInvoices(sessionReferenceNumber: string) {
        const url = `${this.env.KSEF_URL}/sessions/${sessionReferenceNumber}/invoices`;
        const response = await fetch(url, { headers: { Authorization: `Bearer ${this.token}` }});
        const body = await response.text();
        if (!response.ok)
            throw new Error(`KSeF session invoices failed ${response.status}: ${body}`);
        return JSON.parse(body);
    }

    async getInvoiceStatus(sessionReferenceNumber: string, invoiceReferenceNumber: string) {
        const url = `${this.env.KSEF_URL}/sessions/${sessionReferenceNumber}/invoices/${invoiceReferenceNumber}`;
        const response = await fetch(url, { headers: { Authorization: `Bearer ${this.token}` }});
        const body = await response.text();
        if (!response.ok)
            throw new Error(`KSeF invoice status failed ${response.status}: ${body}`);
        return JSON.parse(body);
    }
}