import React, { useState, useEffect } from 'react';
import {
  Container,
  Header,
  Table,
  Box,
  Spinner,
  StatusIndicator,
  TextFilter,
  Pagination,
  CollectionPreferences,
  SpaceBetween,
  Button,
  Modal
} from '@cloudscape-design/components';

function Transactions() {
  const [transactions, setTransactions] = useState([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState(null);
  const [filterText, setFilterText] = useState('');
  const [currentPage, setCurrentPage] = useState(1);
  const [pageSize, setPageSize] = useState(10);

  const [exitModalVisible, setExitModalVisible] = useState(false);
  const [exitTxid, setExitTxid] = useState('');
  const [exitLoading, setExitLoading] = useState(false);
  const [exitError, setExitError] = useState(null);
  const [exitSuccess, setExitSuccess] = useState(null);

  const API_URL = process.env.REACT_APP_API_URL || 'http://localhost:3030/api';

  const fetchTransactions = async () => {
    try {
      setLoading(true);
      console.log('Fetching transactions from:', `${API_URL}/transactions`);
      const response = await fetch(`${API_URL}/transactions`);
      
      console.log('Response status:', response.status);
      if (!response.ok) {
        throw new Error(`Error fetching transactions: ${response.statusText}`);
      }
      
      const data = await response.json();
      console.log('Transactions data:', data);
      setTransactions(data);
      setError(null);
    } catch (err) {
      console.error('Error fetching transactions:', err);
      setError('Failed to load transactions: ' + err.message);
    } finally {
      setLoading(false);
    }
  };

  useEffect(() => {
    fetchTransactions();
  }, []);

  const formatDate = (timestamp) => {
    return new Date(timestamp * 1000).toLocaleString();
  };

  const handleExit = async (txid) => {
    setExitTxid(txid);
    setExitModalVisible(true);
    setExitError(null);
    setExitSuccess(null);
  };

  const performExit = async () => {
    try {
      setExitLoading(true);
      
      const response = await fetch(`${API_URL}/transactions/exit`, {
        method: 'POST',
        headers: {
          'Content-Type': 'application/json',
        },
        body: JSON.stringify({ vtxo_txid: exitTxid }),
      });
      
      if (!response.ok) {
        const errorData = await response.json();
        throw new Error(errorData.error || `Error performing exit: ${response.statusText}`);
      }
      
      const result = await response.json();
      console.log('Exit result:', result);
      
      setExitSuccess('Unilateral exit performed successfully!');
      
      // Refresh transactions after a short delay
      setTimeout(() => {
        fetchTransactions();
        setExitModalVisible(false);
      }, 2000);
    } catch (err) {
      console.error('Exit error:', err);
      setExitError('Failed to perform unilateral exit: ' + err.message);
    } finally {
      setExitLoading(false);
    }
  };

  // Filter transactions based on search text
  const filteredTransactions = transactions.filter(tx => 
    tx.txid.includes(filterText) || 
    tx.type_name.toLowerCase().includes(filterText.toLowerCase())
  );

  // Paginate transactions
  const paginatedTransactions = filteredTransactions.slice(
    (currentPage - 1) * pageSize,
    currentPage * pageSize
  );

  if (loading) {
    return (
      <Container>
        <Spinner size="large" />
      </Container>
    );
  }

  return (
    <Container>
      <SpaceBetween size="l">
        <Header
          variant="h1"
          description="View your transaction history"
        >
          Transactions
        </Header>

        {error && (
          <Box variant="error">
            {error}
          </Box>
        )}

        <Button onClick={fetchTransactions} variant="normal">
          Refresh Transactions
        </Button>

        <Table
          header={
            <Header
              counter={`(${filteredTransactions.length})`}
              actions={
                <TextFilter
                  filteringText={filterText}
                  onChange={({ detail }) => setFilterText(detail.filteringText)}
                  placeholder="Find transactions"
                />
              }
            >
              Transactions
            </Header>
          }
          columnDefinitions={[
            {
              id: 'type',
              header: 'Type',
              cell: item => item.type_name,
              sortingField: 'type_name'
            },
            {
              id: 'amount',
              header: 'Amount (sats)',
              cell: item => (
                <span style={{ color: item.amount > 0 ? 'green' : 'red' }}>
                  {item.amount > 0 ? '+' : ''}{item.amount}
                </span>
              ),
              sortingField: 'amount'
            },
            {
              id: 'date',
              header: 'Date',
              cell: item => formatDate(item.timestamp),
              sortingField: 'timestamp'
            },
            {
              id: 'status',
              header: 'Status',
              cell: item => {
                if (item.is_settled === undefined || item.is_settled === null) {
                  return <StatusIndicator type="error">Cancelled</StatusIndicator>;
                } else if (item.is_settled) {
                  return <StatusIndicator type="success">Settled</StatusIndicator>;
                } else {
                  return <StatusIndicator type="pending">Pending</StatusIndicator>;
                }
              }
            },
            {
              id: 'txid',
              header: 'Transaction ID',
              cell: item => (
                <span title={item.txid}>
                  {item.txid.substring(0, 10)}...
                </span>
              )
            },
            {
              id: 'actions',
              header: 'Actions',
              cell: item => (
                item.is_settled === false ? (
                  <Button
                    onClick={() => handleExit(item.txid)}
                    variant="normal"
                    iconName="external"
                  >
                    Exit
                  </Button>
                ) : null
              )
            }
          ]}
          items={paginatedTransactions}
          pagination={
            <Pagination
              currentPageIndex={currentPage}
              pagesCount={Math.ceil(filteredTransactions.length / pageSize)}
              onChange={({ detail }) => setCurrentPage(detail.currentPageIndex)}
            />
          }
          preferences={
            <CollectionPreferences
              title="Preferences"
              confirmLabel="Confirm"
              cancelLabel="Cancel"
              preferences={{
                pageSize
              }}
              pageSizePreference={{
                title: "Page size",
                options: [
                  { value: 10, label: "10 transactions" },
                  { value: 20, label: "20 transactions" },
                  { value: 50, label: "50 transactions" }
                ]
              }}
              onConfirm={({ detail }) => setPageSize(detail.pageSize)}
            />
          }
          empty={
            <Box textAlign="center" color="inherit">
              <b>No transactions</b>
              <Box padding={{ bottom: "s" }} variant="p" color="inherit">
                No transactions to display.
              </Box>
            </Box>
          }
        />
      <Modal
        visible={exitModalVisible}
        onDismiss={() => setExitModalVisible(false)}
        header="Confirm Unilateral Exit"
        footer={
          <Box float="right">
            <SpaceBetween direction="horizontal" size="xs">
              <Button variant="link" onClick={() => setExitModalVisible(false)}>
                Cancel
              </Button>
              <Button variant="primary" onClick={performExit} loading={exitLoading}>
                Confirm Exit
              </Button>
            </SpaceBetween>
          </Box>
        }
      >
        <SpaceBetween size="m">
          <p>
            Are you sure you want to perform a unilateral exit for this transaction?
            This will broadcast your VTXO on-chain, which may incur network fees.
          </p>
          
          <p>
            <strong>Transaction ID:</strong> {exitTxid}
          </p>
          
          {exitError && (
            <Box variant="error">
              {exitError}
            </Box>
          )}
          
          {exitSuccess && (
            <Box variant="success">
              {exitSuccess}
            </Box>
          )}
        </SpaceBetween>
      </Modal>
      </SpaceBetween>
    </Container>
  );
}

export default Transactions;