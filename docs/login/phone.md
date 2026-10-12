# Login with phone

Phone login requires an existing user. Codes expire after **5 minutes**, allow **3 attempts**, and can be requested once per **60 seconds**. Signed tokens expire after **30 days**; the backend stores no sessions.

```mermaid
sequenceDiagram
    autonumber
    actor User
    participant CLI as Rust CLI
    participant API as worker-api
    participant DB as D1
    participant WA as worker-whatsapp
    participant Meta as Meta / WhatsApp
    participant File as Local session.json

    User->>CLI: Select phone login and enter phone number
    CLI->>API: POST /login/phone {phone}
    API->>DB: Find existing user by normalized phone
    DB-->>API: User ID and language (en / pl)
    API->>API: Generate random six-digit code and HMAC hash
    API->>DB: Insert or replace login_phone_challenges
    Note over API,DB: Store hash, expiry, attempt count and resend cooldown<br/>Reject resend within 60 seconds
    API->>WA: POST /whatsapp/login/phone {phone, code, language}<br/>WHATSAPP service binding, X-API-Key
    WA->>Meta: Send login_phone_code_en or login_phone_code_pl<br/>Same code in message body and copy-code button
    Meta-->>WA: Message accepted
    WA-->>API: Success
    API-->>CLI: 202 {expiresIn: 300}
    Meta-->>User: WhatsApp verification code

    User->>CLI: Enter six-digit code
    CLI->>API: POST /login/phone {phone, code}
    API->>DB: Find user by phone
    DB-->>API: User
    API->>API: Hash submitted code
    API->>DB: Atomically increment attempts and consume on match<br/>Require unexpired, unused challenge with attempts below 3
    alt Incorrect, expired, consumed or exhausted challenge
        DB-->>API: No successful consumption
        API-->>CLI: 401 Invalid or expired verification code
        CLI-->>User: Retry within limit, or request a new code
    else Correct code
        DB-->>API: Challenge consumed
        API->>API: Sign JWT using PHONE_TOKEN_SIGNER<br/>User ID, phone, issuer, audience and 30-day expiry
        API-->>CLI: 200 {token, expiresAt}
        CLI->>CLI: Check token expiry
        CLI->>API: GET /login/phone with Bearer token
        API->>API: Verify signature, issuer, audience and expiry
        API->>DB: Find user matching token user ID and phone
        DB-->>API: User
        API-->>CLI: 200 {phone, email}
        CLI->>File: Save method=phone and token in refresh_token field
        CLI-->>User: Continue into application
    end

    opt Later launch with a saved phone login
        User->>CLI: Select phone login
        CLI->>File: Read saved token
        File-->>CLI: Token
        CLI->>CLI: Check embedded expiry
        alt Token expired or unreadable
            CLI->>File: Remove saved session
            CLI-->>User: Start code login again
        else Token has not expired locally
            CLI->>API: GET /login/phone with Bearer token
            API->>API: Verify signature, issuer, audience and expiry
            API->>DB: Find user matching token user ID and phone
            DB-->>API: Matching user or no match
            alt Valid token and matching account
                API-->>CLI: 200 {phone, email}
                CLI-->>User: Continue without another WhatsApp code
            else Invalid token or missing account
                API-->>CLI: 401 or 404
                CLI->>File: Remove saved session
                CLI-->>User: Start code login again
            end
        end
    end

    opt Authenticated application request
        CLI->>API: API request with existing API key, Cloudflare credentials and X-User-Id
        API->>API: Apply existing API authentication
        API->>DB: Resolve application user
        DB-->>API: User
        API-->>CLI: Requested result
    end
```
