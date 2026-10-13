export class AuthError extends Error {
    constructor(message: string, public status = 404, public details?: unknown) {
        super(message);
        this.name = "AuthError";
    }
}

export type PhoneLoginChallenge = {
    phone: string;
    codeHash: string;
    expiresAt: string;
    attempts: number;
    resendAfter: string;
    updatedAt?: string;
}