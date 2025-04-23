import React, { useState } from 'react';
import {
  Container,
  Header,
  SpaceBetween,
  FormField,
  Input,
  Button,
  Alert,
  Box,
  ColumnLayout
} from '@cloudscape-design/components';

function Receive() {
  const [amount, setAmount] = useState('');
  const [fromAddress, setFromAddress] = useState('');
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState(null);
  const [success, setSuccess] = useState(null);
  const [txDetails, setTxDetails] = useState(null);

  const API_URL = process.env.REACT_APP_API_URL || 'http://localhost:3030/api';

  const handleSubmit = async (e) => {
    e.preventDefault();
    
    // Validation
    if (!fromAddress) {
      setError('Please enter a sender address');
      return;
    }
    
    if (!amount) {
      setError('Please enter an amount');
      return;
    }
    
    const amountSats = parseInt(amount, 10);
    if (isNaN(amountSats) || amountSats <= 0) {
      setError('Amount must be a positive number');
      return;
    }

    try {
      setLoading(true);
      setError(null);
      setSuccess(null);
      setTxDetails(null);
      
      console.log(`Receiving ${amountSats} sats from ${fromAddress}`);
      
      const response = await fetch(`${API_URL}/wallet/receive`, {
        method: 'POST',
        headers: {
          'Content-Type': 'application/json',
        },
        body: JSON.stringify({ from_address: fromAddress, amount: amountSats }),
      });
      
      if (!response.ok) {
        const errorData = await response.json();
        throw new Error(errorData.error || `Error receiving VTXO: ${response.statusText}`);
      }
      
      const result = await response.json();
      console.log('Received VTXO:', result);
      
      setSuccess(`VTXO received successfully!`);
      setTxDetails(result);
      
      // Clear form
      setFromAddress('');
      setAmount('');
    } catch (err) {
      console.error('Receive error:', err);
      setError('Failed to receive VTXO: ' + err.message);
    } finally {
      setLoading(false);
    }
  };

  return (
    <Container>
      <SpaceBetween size="l">
        <Header
          variant="h1"
          description="Receive Bitcoin using the Ark protocol"
        >
          Receive VTXOs
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
              label="Sender Address"
              description="Enter the Ark address of the sender"
              errorText={!fromAddress && error?.includes('sender') ? 'Sender address is required' : undefined}
            >
              <Input
                value={fromAddress}
                onChange={({ detail }) => setFromAddress(detail.value)}
                placeholder="Enter Ark address (e.g., ark1...)"
                disabled={loading}
              />
            </FormField>
            
            <FormField
              label="Amount (sats)"
              description="Enter the amount in satoshis"
              errorText={!amount && error?.includes('amount') ? 'Amount is required' : undefined}
            >
              <Input
                value={amount}
                onChange={({ detail }) => setAmount(detail.value)}
                placeholder="Enter amount in satoshis (e.g., 10000)"
                type="number"
                disabled={loading}
              />
            </FormField>
            
            <Button
              variant="primary"
              formAction="submit"
              loading={loading}
            >
              Receive
            </Button>
          </SpaceBetween>
        </form>
        
        {txDetails && (
          <Container
            header={
              <Header variant="h2">
                Transaction Details
              </Header>
            }
          >
            <ColumnLayout columns={1} variant="text-grid">
              <div>
                <Box variant="awsui-key-label">Transaction ID</Box>
                <div>{txDetails.txid}</div>
              </div>
              
              <div>
                <Box variant="awsui-key-label">Amount</Box>
                <div>{txDetails.amount} sats</div>
              </div>
              
              <div>
                <Box variant="awsui-key-label">Sender</Box>
                <div>{fromAddress}</div>
              </div>
              
              <div>
                <Box variant="awsui-key-label">Status</Box>
                <div>{txDetails.is_settled ? 'Settled' : 'Pending'}</div>
              </div>
            </ColumnLayout>
          </Container>
        )}
        
        <Container
          header={
            <Header variant="h2">
              Testing Instructions
            </Header>
          }
        >
          <SpaceBetween size="m">
            <p>To test this functionality:</p>
            
            <ol>
              <li>Enter a test Ark address (e.g., <code>ark1testaddress123456789</code>)</li>
              <li>Enter an amount in satoshis (e.g., <code>10000</code>)</li>
              <li>Click the Receive button</li>
              <li>Verify that you receive a success message with transaction details</li>
              <li>Check your balance and transaction history</li>
            </ol>
            
            <p><strong>Note:</strong> This is currently using dummy data. In a production environment, you would receive actual Bitcoin transactions.</p>
          </SpaceBetween>
        </Container>
      </SpaceBetween>
    </Container>
  );
}

export default Receive;