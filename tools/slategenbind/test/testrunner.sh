#!/bin/sh

outline() {
echo >>${LOGFILE}
echo "-----------------------------------------------------------"  >>${LOGFILE}
echo >>${LOGFILE}
}

BUILDDIR=$1
TESTSRCDIR=$2

# locations
# test output
TESTOUTDIR=${BUILDDIR}/test
# test overall output
LOGFILE=${TESTOUTDIR}/testlog

# genbind tool
SLATEGENBIND=${BUILDDIR}/slategenbind

#bindings
BINDINGDIR=${TESTSRCDIR}/data/bindings
BINDINGTESTS="${BINDINGDIR}/blankidl.bnd
${BINDINGDIR}/emptyidl.bnd
${BINDINGDIR}/browser-quickjs.bnd"

IDLDIR=${TESTSRCDIR}/data/idl
FAILURES=0

mkdir -p ${TESTOUTDIR}

echo "$*" >${LOGFILE}

for TEST in ${BINDINGTESTS};do

  outline

  TESTNAME=$(basename ${TEST} .bnd)
  TESTDIR=${TESTOUTDIR}/${TESTNAME}

  echo -n "    TEST: ${TESTNAME}......"
  echo "    TEST: ${TESTNAME}......" >>${LOGFILE}

  mkdir -p ${TESTDIR}
  # per test results
  RESFILE=${TESTDIR}/testres
  # per test errors
  ERRFILE=${TESTDIR}/testerr

  echo  ${SLATEGENBIND} -v -D -g -I ${IDLDIR} ${TEST} ${TESTOUTDIR}/${TESTNAME} >>${LOGFILE} 2>&1

  ${SLATEGENBIND} -v -D -g -I ${IDLDIR} ${TEST} ${TESTOUTDIR}/${TESTNAME} >${RESFILE} 2>${ERRFILE}

  RESULT=$?

  echo >> ${LOGFILE}
  cat ${ERRFILE} >> ${LOGFILE}
  echo >> ${LOGFILE}
  cat ${RESFILE} >> ${LOGFILE}

  if [ ${RESULT} -eq 0 ]; then
    echo "PASS"
  else
    echo "FAIL"
    FAILURES=$((FAILURES + 1))
  fi


done

exit ${FAILURES}
