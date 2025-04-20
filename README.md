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
by default Nigiri's Esplora client uses a `port-5000:5000` change that to `port-3002:5000`. If using default port numbers for Esplora client update the enviroment variable in backend directory to match your esplora server address.

### Setup
1. Clone the repo 
```
git clone https://github.com/pingu-73/ark-web-app.git
cd ark-web-app
```

2. Start Nigiri with Ark support
```
nigiri start --Ark
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

## API Documentation
The backend provides the following API endpoints:
- `GET /api/wallet/info`: Get wallet information
- `GET /api/wallet/balance`: Get wallet balance
- `GET /api/wallet/address`: Get an Ark address for receiving off-chain payments
- `GET /api/wallet/boarding-address`: Get a Bitcoin address for receiving on-chain payments
- `POST /api/wallet/send`: Send a payment (either Ark or on-chain)
- `GET /api/transactions`: Get transaction history
- `POST /api/round/participate`: Participate in a round
