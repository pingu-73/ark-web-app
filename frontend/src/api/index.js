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

// Address APIs
export const getArkAddress = async () => {
  const response = await fetch(`${API_URL}/wallet/address`);
  if (!response.ok) {
    throw new Error(`Error fetching Ark address: ${response.statusText}`);
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

export const getOnchainAddress = async () => {
  const response = await fetch(`${API_URL}/wallet/onchain-address`);
  if (!response.ok) {
    throw new Error(`Error fetching onchain address: ${response.statusText}`);
  }
  return response.json();
};

// Balance APIs
export const getAvailableBalance = async () => {
  const response = await fetch(`${API_URL}/wallet/available-balance`);
  if (!response.ok) {
    throw new Error(`Error fetching available balance: ${response.statusText}`);
  }
  return response.json();
};

export const getOnchainBalance = async () => {
  const response = await fetch(`${API_URL}/wallet/onchain-balance`);
  if (!response.ok) {
    throw new Error(`Error fetching onchain balance: ${response.statusText}`);
  }
  return response.json();
};

// Send APIs
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

export const sendOnchainPayment = async (address, amount, priority = 'normal') => {
  const response = await fetch(`${API_URL}/wallet/send-onchain`, {
    method: 'POST',
    headers: {
      'Content-Type': 'application/json',
    },
    body: JSON.stringify({ address, amount, priority }),
  });
  if (!response.ok) {
    const errorText = await response.text();
    throw new Error(`Error sending onchain payment: ${response.status} ${response.statusText} - ${errorText}`);
  }
  return response.json();
};

// Fee estimation
export const getFeeEstimates = async () => {
  const response = await fetch(`${API_URL}/wallet/fee-estimates`);
  if (!response.ok) {
    const errorText = await response.text();
    throw new Error(`Error fetching fee estimates: ${response.status} ${response.statusText} - ${errorText}`);
  }
  return response.json();
};

export const estimateTransactionFees = async (address, amount) => {
  const response = await fetch(`${API_URL}/wallet/estimate-transaction-fees`, {
    method: 'POST',
    headers: {
      'Content-Type': 'application/json',
    },
    body: JSON.stringify({ address, amount }),
  });
  if (!response.ok) {
    const errorText = await response.text();
    throw new Error(`Error estimating fees: ${response.status} ${response.statusText} - ${errorText}`);
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