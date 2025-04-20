const API_URL = process.env.REACT_APP_API_URL || 'http://localhost:3030/api';

// Wallet API
export const getWalletInfo = async () => {
  const response = await fetch(`${API_URL}/wallet/info`);
  if (!response.ok) {
    throw new Error(`Error fetching wallet info: ${response.statusText}`);
  }
  return response.json();
};

export const getWalletBalance = async () => {
  const response = await fetch(`${API_URL}/wallet/balance`);
  if (!response.ok) {
    throw new Error(`Error fetching wallet balance: ${response.statusText}`);
  }
  return response.json();
};

export const getAddress = async () => {
  const response = await fetch(`${API_URL}/wallet/address`);
  if (!response.ok) {
    throw new Error(`Error fetching address: ${response.statusText}`);
  }
  return response.json();
};

export const getBoardingAddress = async () => {
  const response = await fetch(`${API_URL}/wallet/boarding-address`);
  if (!response.ok) {
    throw new Error(`Error fetching boarding address: ${response.statusText}`);
  }
  return response.json();
};

export const sendVtxo = async (address, amount) => {
  const response = await fetch(`${API_URL}/wallet/send`, {
    method: 'POST',
    headers: {
      'Content-Type': 'application/json',
    },
    body: JSON.stringify({ address, amount }),
  });
  if (!response.ok) {
    throw new Error(`Error sending VTXO: ${response.statusText}`);
  }
  return response.json();
};

// Transaction API
export const getTransactionHistory = async () => {
  const response = await fetch(`${API_URL}/transactions`);
  if (!response.ok) {
    throw new Error(`Error fetching transactions: ${response.statusText}`);
  }
  return response.json();
};

export const participateInRound = async () => {
  const response = await fetch(`${API_URL}/round/participate`, {
    method: 'POST',
  });
  if (!response.ok) {
    throw new Error(`Error participating in round: ${response.statusText}`);
  }
  return response.json();
};