# Login with Microsoft

The CLI signs in directly with Microsoft using the authorization code flow with PKCE. The current implementation targets personal Microsoft accounts through the `consumers` endpoint.

```mermaid
sequenceDiagram
    autonumber
    actor User
    participant CLI as Rust CLI
    participant Browser
    participant MS as Microsoft identity platform
    participant File as Local session.json
    participant API as worker-api
    participant DB as D1

    User->>CLI: Select Microsoft login
    CLI->>File: Read saved session
    File-->>CLI: Session or no saved session

    opt Saved method is microsoft
        CLI->>MS: POST /consumers/oauth2/v2.0/token<br/>grant_type=refresh_token, client_id, refresh_token, scopes
        alt Refresh succeeds
            MS-->>CLI: ID token and optional replacement refresh token
            opt Replacement refresh token returned
                CLI->>File: Save method=microsoft and refresh_token
            end
            CLI->>CLI: Decode ID token payload into user identity
        else Refresh fails
            MS-->>CLI: Error
            CLI->>File: Remove saved session
            Note over User,CLI: Continue with interactive sign-in
        end
    end

    opt No Microsoft session could be resumed
        CLI->>CLI: Listen on 127.0.0.1 with an available port<br/>Generate random state and PKCE verifier<br/>Derive S256 code challenge
        CLI->>Browser: Open Microsoft authorization URL<br/>client_id, redirect_uri, scopes, state, code_challenge
        Browser->>MS: GET /consumers/oauth2/v2.0/authorize
        MS-->>Browser: Sign-in and consent screen
        User->>Browser: Sign in and approve
        MS-->>Browser: Redirect to http://localhost:port with code and state
        Browser->>CLI: Local OAuth callback
        CLI-->>Browser: Close-window message
        CLI->>CLI: Check provider errors, matching state and authorization code
        CLI->>MS: POST /consumers/oauth2/v2.0/token<br/>grant_type=authorization_code, client_id, code, redirect_uri, code_verifier
        MS-->>CLI: ID token and optional refresh token
        opt Refresh token returned
            CLI->>File: Save method=microsoft and refresh_token
        end
        CLI->>CLI: Decode ID token payload into user identity
    end

    CLI->>API: GET /app/users?email=...&onboarding=true<br/>Cloudflare Access credentials and X-API-Key
    API->>DB: Find application user by email
    alt Existing application user
        DB-->>API: User record
        API-->>CLI: 200 with user record
        CLI-->>User: Open application menu
    else No application user
        DB-->>API: No match
        API-->>CLI: 404
        CLI->>CLI: Call init_app_user with Microsoft identity
        Note over User,CLI: First-time setup is currently a local placeholder
    end
```