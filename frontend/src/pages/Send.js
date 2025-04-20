import React, { useState } from 'react';
import {
  Container,
  Header,
  SpaceBetween,
  FormField,
  Input,
  Button,
  Alert
} from '@cloudscape-design/components';

function Send() {
  const [address, setAddress] = useState('');
  const [amount, setAmount] = useState('');
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState(null);
  const [success, setSuccess] = useState(null);

  const API_URL = process.env.REACT_APP_API_URL || 'http://localhost:3030/api';

  const handleSubmit = async (e) => {
    e.preventDefault();
    
    if (!address || !amount) {
      setError('Please fill in all fields');
      return;
    }

    try {
      setLoading(true);
      setError(null);
      setSuccess(null);
      
      const amountSats = parseInt(amount, 10);
      if (isNaN(amountSats) || amountSats <= 0) {
        throw new Error('Amount must be a positive number');
      }
      
      const response = await fetch(`${API_URL}/wallet/send`, {
        method: 'POST',
        headers: {
          'Content-Type': 'application/json',
        },
        body: JSON.stringify({ address, amount: amountSats }),
      });
      
      if (!response.ok) {
        throw new Error(`Error sending transaction: ${response.statusText}`);
      }
      
      const result = await response.json();
      setSuccess(`Transaction sent successfully! TXID: ${result.txid}`);
      
      // Clear form
      setAddress('');
      setAmount('');
    } catch (err) {
      setError('Failed to send transaction: ' + err.message);
    } finally {
      setLoading(false);
    }
  };

  return (
    <Container>
      <SpaceBetween size="l">
        <Header
          variant="h1"
        >
          Send VTXOs
        </Header>
        
        {error && (
          <Alert type="error" header="Error">
            {error}
          </Alert>
        )}
        
        {success && (
          <Alert type="success" header="Success">
            {success}
          </Alert>
        )}
        
        <form onSubmit={handleSubmit}>
          <SpaceBetween size="l">
            <FormField
              label="Recipient Address"
              description="Enter the Ark address of the recipient"
            >
              <Input
                value={address}
                onChange={({ detail }) => setAddress(detail.value)}
                placeholder="Enter Ark address"
                disabled={loading}
              />
            </FormField>
            
            <FormField
              label="Amount (sats)"
              description="Enter the amount in satoshis"
            >
              <Input
                value={amount}
                onChange={({ detail }) => setAmount(detail.value)}
                placeholder="Enter amount in satoshis"
                type="number"
                disabled={loading}
              />
            </FormField>
            
            <Button
              variant="primary"
              formAction="submit"
              loading={loading}
            >
              Send
            </Button>
          </SpaceBetween>
        </form>
      </SpaceBetween>
    </Container>
  );
}

export default Send;