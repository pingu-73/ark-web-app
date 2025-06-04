import React, { useState, useEffect } from 'react';
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
  Box,
  Table,
  ColumnLayout
} from '@cloudscape-design/components';
import { sendVtxo, sendOnchainPayment, estimateTransactionFees, getFeeEstimates } from '../api';

function Send() {
  const [activeTab, setActiveTab] = useState('vtxo');
  const [address, setAddress] = useState('');
  const [amount, setAmount] = useState('');
  const [priority, setPriority] = useState({ label: 'Normal', value: 'normal' });
  const [generalFeeEstimates, setGeneralFeeEstimates] = useState(null);
  const [transactionFeeEstimates, setTransactionFeeEstimates] = useState(null);
  const [loading, setLoading] = useState(false);
  const [estimatingFee, setEstimatingFee] = useState(false);
  const [loadingGeneralFees, setLoadingGeneralFees] = useState(false);
  const [error, setError] = useState(null);
  const [success, setSuccess] = useState(null);

  const priorityOptions = [
    { label: 'Fastest (~10 minutes)', value: 'fastest' },
    { label: 'Fast (~30 minutes)', value: 'fast' },
    { label: 'Normal (~1 hour)', value: 'normal' },
    { label: 'Slow (~2-24 hours)', value: 'slow' }
  ];

  // Load general fee estimates when component mounts or tab changes to onchain
  useEffect(() => {
    if (activeTab === 'onchain') {
      loadGeneralFeeEstimates();
    }
  }, [activeTab]);

  const loadGeneralFeeEstimates = async () => {
    try {
      setLoadingGeneralFees(true);
      const feeData = await getFeeEstimates();
      setGeneralFeeEstimates(feeData);
    } catch (err) {
      console.warn('Failed to load general fee estimates:', err.message);
    } finally {
      setLoadingGeneralFees(false);
    }
  };

  const handleEstimateTransactionFee = async (e) => {
    e.preventDefault(); // Prevent form submission
    e.stopPropagation(); // Stop event bubbling
    
    if (!address || !amount) {
      setError('Please fill in address and amount first');
      return;
    }

    try {
      setEstimatingFee(true);
      setError(null);
      const amountSats = parseInt(amount, 10);
      if (isNaN(amountSats) || amountSats <= 0) {
        throw new Error('Amount must be a positive number');
      }
      
      const feeData = await estimateTransactionFees(address, amountSats);
      setTransactionFeeEstimates(feeData);
    } catch (err) {
      setError('Failed to estimate transaction fee: ' + err.message);
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
        result = await sendOnchainPayment(address, amountSats, priority.value);
        setSuccess(`On-chain payment sent successfully! TXID: ${result.txid}`);
      }
      
      // Clear form
      setAddress('');
      setAmount('');
      setTransactionFeeEstimates(null);
    } catch (err) {
      setError('Failed to send transaction: ' + err.message);
    } finally {
      setLoading(false);
    }
  };

  const resetForm = () => {
    setAddress('');
    setAmount('');
    setTransactionFeeEstimates(null);
    setError(null);
    setSuccess(null);
    setPriority({ label: 'Normal', value: 'normal' });
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
            resetForm();
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
                      onClick={handleSubmit}
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
                <SpaceBetween size="l">
                  <Alert type="info">
                    Send regular Bitcoin transactions to any Bitcoin address.
                  </Alert>

                  {/* General Network Fee Rates */}
                  {generalFeeEstimates && (
                    <Container
                      header={
                        <Header variant="h3">
                          Current Network Fee Rates
                          <Button
                            iconName="refresh"
                            onClick={loadGeneralFeeEstimates}
                            loading={loadingGeneralFees}
                            variant="icon"
                          />
                        </Header>
                      }
                    >
                      <Table
                        columnDefinitions={[
                          {
                            id: 'priority',
                            header: 'Priority',
                            cell: (item) => item.priority
                          },
                          {
                            id: 'rate',
                            header: 'Fee Rate (sat/vB)',
                            cell: (item) => item.rate
                          }
                        ]}
                        items={[
                          { priority: 'Fastest', rate: generalFeeEstimates.fastest },
                          { priority: 'Fast', rate: generalFeeEstimates.fast },
                          { priority: 'Normal', rate: generalFeeEstimates.normal },
                          { priority: 'Slow', rate: generalFeeEstimates.slow }
                        ]}
                        variant="embedded"
                      />
                    </Container>
                  )}
                  
                  {/* Transaction Form */}
                  <Container
                    header={<Header variant="h3">Transaction Details</Header>}
                  >
                    <SpaceBetween size="l">
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

                      <FormField
                        label="Priority"
                        description="Select transaction priority (affects fee and confirmation time)"
                      >
                        <Select
                          selectedOption={priority}
                          onChange={({ detail }) => setPriority(detail.selectedOption)}
                          options={priorityOptions}
                          disabled={loading}
                        />
                      </FormField>
                      
                      {/* Fee Estimation - Outside of form */}
                      <SpaceBetween size="m">
                        <Button
                          onClick={handleEstimateTransactionFee}
                          loading={estimatingFee}
                          disabled={!address || !amount}
                          variant="normal"
                        >
                          Estimate Transaction Fee
                        </Button>
                        
                        {transactionFeeEstimates && (
                          <Container
                            header={
                              <Header variant="h4">Transaction Fee Estimate</Header>
                            }
                          >
                            <SpaceBetween size="m">
                              <Table
                                columnDefinitions={[
                                  {
                                    id: 'priority',
                                    header: 'Priority',
                                    cell: item => item.priority
                                  },
                                  {
                                    id: 'time',
                                    header: 'Est. Time',
                                    cell: item => item.blocks
                                  },
                                  {
                                    id: 'fee_rate',
                                    header: 'Fee Rate',
                                    cell: item => `${item.fee_rate} sat/vB`
                                  },
                                  {
                                    id: 'fee',
                                    header: 'Total Fee',
                                    cell: item => `${item.total_fee} sats`
                                  }
                                ]}
                                items={transactionFeeEstimates.transaction_fees}
                                variant="embedded"
                              />
                              
                              <Box>
                                <strong>
                                  Selected Priority ({priority.label}): {
                                    transactionFeeEstimates.transaction_fees.find(
                                      fee => fee.priority === priority.value
                                    )?.total_fee || 'N/A'
                                  } sats
                                </strong>
                              </Box>
                            </SpaceBetween>
                          </Container>
                        )}
                      </SpaceBetween>
                      
                      {/* Send Button - Separate from estimation */}
                      <form onSubmit={handleSubmit}>
                        <Button
                          variant="primary"
                          onClick={handleSubmit}
                          loading={loading}
                          disabled={!address || !amount}
                        >
                          Send On-chain Transaction
                        </Button>
                      </form>
                    </SpaceBetween>
                  </Container>
                </SpaceBetween>
              )
            }
          ]}
        />
      </SpaceBetween>
    </Container>
  );
}

export default Send;