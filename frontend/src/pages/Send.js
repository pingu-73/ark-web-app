import React, { useState } from 'react';
import {
  Container,
  Header,
  SpaceBetween,
  FormField,
  Input,
  Button,
  Alert,
  Tabs,
  Select,
  Box
} from '@cloudscape-design/components';
import { sendVtxo, sendOnchainPayment, estimateFee } from '../api';

function Send() {
  const [activeTab, setActiveTab] = useState('vtxo');
  const [address, setAddress] = useState('');
  const [amount, setAmount] = useState('');
  const [estimatedFee, setEstimatedFee] = useState(null);
  const [loading, setLoading] = useState(false);
  const [estimatingFee, setEstimatingFee] = useState(false);
  const [error, setError] = useState(null);
  const [success, setSuccess] = useState(null);

  const handleEstimateFee = async () => {
    if (!address || !amount) {
      setError('Please fill in address and amount first');
      return;
    }

    try {
      setEstimatingFee(true);
      const amountSats = parseInt(amount, 10);
      const feeData = await estimateFee(address, amountSats);
      setEstimatedFee(feeData.estimated_fee);
    } catch (err) {
      setError('Failed to estimate fee: ' + err.message);
    } finally {
      setEstimatingFee(false);
    }
  };

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
      
      let result;
      if (activeTab === 'vtxo') {
        result = await sendVtxo(address, amountSats);
        setSuccess(`VTXO sent successfully! TXID: ${result.txid}`);
      } else {
        result = await sendOnchainPayment(address, amountSats);
        setSuccess(`On-chain payment sent successfully! TXID: ${result.txid}`);
      }
      
      // Clear form
      setAddress('');
      setAmount('');
      setEstimatedFee(null);
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
          description="Send Bitcoin using Ark or regular on-chain transactions"
        >
          Send Bitcoin
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
        
        <Tabs
          activeTabId={activeTab}
          onChange={({ detail }) => {
            setActiveTab(detail.activeTabId);
            setError(null);
            setSuccess(null);
            setEstimatedFee(null);
          }}
          tabs={[
            {
              label: "Send VTXO (Off-chain)",
              id: "vtxo",
              content: (
                <form onSubmit={handleSubmit}>
                  <SpaceBetween size="l">
                    <Alert type="info">
                      Send VTXOs instantly to other Ark users with minimal fees.
                    </Alert>
                    
                    <FormField
                      label="Recipient Ark Address"
                      description="Enter the Ark address of the recipient"
                    >
                      <Input
                        value={address}
                        onChange={({ detail }) => setAddress(detail.value)}
                        placeholder="Enter Ark address (ark1...)"
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
                      Send VTXO
                    </Button>
                  </SpaceBetween>
                </form>
              )
            },
            {
              label: "Send On-chain",
              id: "onchain",
              content: (
                <form onSubmit={handleSubmit}>
                  <SpaceBetween size="l">
                    <Alert type="info">
                      Send regular Bitcoin transactions to any Bitcoin address.
                    </Alert>
                    
                    <FormField
                      label="Recipient Bitcoin Address"
                      description="Enter any valid Bitcoin address"
                    >
                      <Input
                        value={address}
                        onChange={({ detail }) => setAddress(detail.value)}
                        placeholder="Enter Bitcoin address (bc1... or bcrt1...)"
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
                    
                    <SpaceBetween size="m" direction="horizontal">
                      <Button
                        onClick={handleEstimateFee}
                        loading={estimatingFee}
                        disabled={!address || !amount}
                      >
                        Estimate Fee
                      </Button>
                      
                      {estimatedFee && (
                        <Box>
                          <strong>Estimated Fee: {estimatedFee} sats</strong>
                        </Box>
                      )}
                    </SpaceBetween>
                    
                    <Button
                      variant="primary"
                      formAction="submit"
                      loading={loading}
                    >
                      Send On-chain
                    </Button>
                  </SpaceBetween>
                </form>
              )
            }
          ]}
        />
      </SpaceBetween>
    </Container>
  );
}

export default Send;