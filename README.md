# Ark Web Wallet
A web-based Bitcoin wallet with Ark protocol integration for off-chain transactions.

## overview
Ark Web Wallet is a full-stack web application that allows users to:
- Create and manage Bitcoin wallets
- Send and receive on-chain Bitcoin transactions
- Send and receive off-chain transactions using the Ark protocol
- View transaction history
- Participate in settlement rounds

The application consists of a Rust backend API and a React frontend, providing a seamless user experience for managing Bitcoin with Ark protocol integration.

![backend-server](./assets/backend.png)

## Features
- Wallet Management: Create and access Bitcoin wallets
- On-chain Transactions: Send and receive regular Bitcoin transactions
- Off-chain Transactions: Use the Ark protocol for faster, cheaper transactions
- Transaction History: View all on-chain and off-chain transactions
- Round Participation: Participate in settlement rounds to batch transactions
- Balance Tracking: Monitor confirmed and pending balances
- Address Generation: Generate addresses for receiving funds

## Architecture
### Backend
The backend is built with Rust and provides a RESTful for the frontend and used Grpc to connect with Ark server. It uses:
- Axum: Web framework for handling HTTP requests
- Ark Client: Integration with the Ark protocol
- Bitcoin Development Kit: For Bitcoin wallet functionality
- Tokio: Asynchronous runtime

### Frontend
The frontend is built with React and provides a user-friendly interface. It uses:
- React: JavaScript library for building user interfaces
- React Router: For navigation between pages
- Cloudscape Design System: For UI components
- Fetch API: For communication with the backend

## Getting Started
### Prerequisites
- Rust
- Node.js
- [Nigiri](https://nigiri.vulpem.com/) for server side
To download nigiri run the following command:
```
curl https://getnigiri.vulpem.com | bash
```
by default Nigiri's Esplora frontend client uses a `port-5000:5000` change that to `port-5050:5000`. If using default port numbers for Esplora client update the enviroment variable in backend directory to match your esplora server address.

### Setup
1. Clone the repo 
```
git clone https://github.com/pingu-73/ark-web-app.git
cd ark-web-app
```

2. Start Nigiri with Ark support
```
nigiri start --ark
```

3. Build and run the backend
```
cd backend
cargo build
cargo run
```
The backend will be available at http://localhost:3030

4. Install frontend dependencies and start the development server
```
cd frontend
npm install
npm start
```
The frontend will be available at http://localhost:3000.

## Development
### Backend Development
The backend is structured as follows:
- `src/main.rs`: Entry point and server setup
- `src/api/`: API routes and handlers
- `src/services/`: Business logic
- `src/models/`: Data models

### Frontend Development
The frontend is structured as follows:
- `src/App.js`: Main application component
- `src/pages/`: Page components
- `src/components/`: Reusable UI components
- `src/api/`: API client

# Wallet APIs
### GET /api/wallet/info
- Returns information about the wallet, including network, server URL, and connection status.
**Example:**
```
~
❯ curl http://localhost:3030/api/wallet/info
{"network":"regtest","server_url":"http://localhost:7070","connected":true}
```

### GET /api/wallet/available-balance
- Returns the available (confirmed) balance that can be spent.
**Example:**
```
❯ curl http://localhost:3030/api/wallet/available-balance
{"available":1100000}
```

### GET /api/wallet/recalculate-balance 
- Returns the wallet balance, including confirmed, pending, and total amounts.
**Example:**
```
❯ curl -X POST http://localhost:3030/api/wallet/recalculate-balance
{"confirmed":1100000,"trusted_pending":0,"untrusted_pending":0,"immature":0,"total":1100000}
```

### GET /api/transactions
- Returns the transaction history.
**Example:** 
```
❯ curl http://localhost:3030/api/transactions
[{"txid":"a3a1838f320fbd9e02cb8aa808f9308ba07a676a75787e6b8b1387abb3c6a885","amount":100000,"timestamp":1747820540,"type_name":"Boarding","is_settled":true},{"txid":"e3f0b8769a355543307e58ea34c9725330709e61e737e66f45c8149758843316","amount":1000000,"timestamp":1747820900,"type_name":"Boarding","is_settled":true}]
```

> **NOTE:** Few of the above mentioned API's are still not connected to the frontend but can be used as CLI. Removal of unused/wrong api's from [main.rs](./backend/src/main.rs) is still remaining.

# Reamining Work
1. **Commit Reveal protocol**
- [ ] Add models for commit and reveal transactions
- [ ] Create API endpoints for the game flow
- [ ] Implement service functions for game logic

2. **Game State Management**
- [ ] Track game states (waiting for commit, waiting for reveal, completed)
- [ ] Handle timeouts and disputes
- [ ] Manage game history

3. **Features**
- [ ] proper random number generation
- [ ] verification of commitments and reveals

4. **Crate with Core functionality for other developers to use**

5. **Features**
- [ ] VTXO Tree Implementation
- [ ] Connector Mechanism (to ensure atomicity b/w forfeit txs and round txs)
- [ ] Timelock Handling (for boarding outputs and unilateral exits)
- [ ] Batch Expiry (track and handle batch expiry for liquidity recycling)

6. **Dummy to real impl**
- [x] make actual gRPC calls to the Ark server for balances, addresses, and tx history
- [ ] add signing and verification steps in Round participation

7. **Security Fatures (Improvement from current impl)**
- [ ] tx Broadcasting (currently not broadcasting txs to network)
- [ ] bitcoin blockchain interaction for on-chain tx
- [ ] UTXO management
- [x] Key Management
- [ ] Signature Verification
- [ ] Add cryptographic op for protocol's security
- [ ] Implement taproot script with collaborative and exit paths