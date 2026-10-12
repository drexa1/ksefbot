export class AuthError extends Error {
    constructor(message: string, public status = 404, public details?: unknown) {
        super(message);
        this.name = "AuthError";
    }
}

export type PhoneLoginChallenge = {
    phone: string;
    codeHash: string;
    expiresAt: number;
    attempts: number;
    resendAfter: number;
    consumedAt: number | null;
}